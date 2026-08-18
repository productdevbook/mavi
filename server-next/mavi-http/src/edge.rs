//! Request-source extraction and bounded edge throttling.
//!
//! The edge signal is deliberately kept outside the domain crates. IP
//! addresses and user-agent strings are transport input, not site data. Only
//! a keyed digest reaches the in-memory limiter and the security audit event.

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{HeaderName, Method, Request, header::USER_AGENT},
};
use base64::Engine;
use mavi_core::{MaviError, SiteId};
use sha2::{Digest, Sha256};

const FORWARDED: HeaderName = HeaderName::from_static("forwarded");
const X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");
const MAX_USER_AGENT_BYTES: usize = 512;
const DEFAULT_IP_LIMIT: u32 = 30;
const DEFAULT_DEVICE_LIMIT: u32 = 10;
const DEFAULT_IP_WINDOW: Duration = Duration::from_mins(1);
const DEFAULT_DEVICE_WINDOW: Duration = Duration::from_mins(10);
const DEFAULT_MAX_BUCKETS: usize = 100_000;

/// Public authentication operations protected by the edge limiter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EdgeAction {
    SessionCreate,
    PasswordResetRequest,
    PasswordResetRedeem,
    EmailVerificationRequest,
    EmailVerificationRedeem,
}

impl EdgeAction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SessionCreate => "auth.session.create",
            Self::PasswordResetRequest => "auth.password_reset.request",
            Self::PasswordResetRedeem => "auth.password_reset.redeem",
            Self::EmailVerificationRequest => "auth.email_verification.request",
            Self::EmailVerificationRedeem => "auth.email_verification.redeem",
        }
    }
}

/// Fixed-window limits applied independently to an IP and a device signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EdgeThrottlePolicy {
    pub ip_limit: u32,
    pub ip_window: Duration,
    pub device_limit: u32,
    pub device_window: Duration,
    pub max_buckets: usize,
}

impl Default for EdgeThrottlePolicy {
    fn default() -> Self {
        Self {
            ip_limit: DEFAULT_IP_LIMIT,
            ip_window: DEFAULT_IP_WINDOW,
            device_limit: DEFAULT_DEVICE_LIMIT,
            device_window: DEFAULT_DEVICE_WINDOW,
            max_buckets: DEFAULT_MAX_BUCKETS,
        }
    }
}

/// A trusted proxy network accepted for forwarded client-IP headers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IpNetwork {
    address: IpAddr,
    prefix: u8,
}

impl IpNetwork {
    fn parse(value: &str) -> Result<Self, MaviError> {
        let (address, prefix) = value.trim().split_once('/').map_or_else(
            || (value.trim(), None),
            |(address, prefix)| (address, Some(prefix)),
        );
        let address = address
            .parse::<IpAddr>()
            .map_err(|_| MaviError::validation("invalid_trusted_proxy_cidr"))?;
        let max_prefix = match address {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        let prefix = prefix
            .map(|prefix| {
                prefix
                    .parse::<u8>()
                    .map_err(|_| MaviError::validation("invalid_trusted_proxy_cidr"))
            })
            .transpose()?
            .unwrap_or(max_prefix);
        if prefix > max_prefix {
            return Err(MaviError::validation("invalid_trusted_proxy_cidr"));
        }
        Ok(Self { address, prefix })
    }

    fn contains(self, candidate: IpAddr) -> bool {
        match (self.address, candidate) {
            (IpAddr::V4(network), IpAddr::V4(candidate)) => {
                let prefix = u32::from(self.prefix);
                let mask = if prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - prefix)
                };
                (u32::from(network) & mask) == (u32::from(candidate) & mask)
            }
            (IpAddr::V6(network), IpAddr::V6(candidate)) => {
                let prefix = u32::from(self.prefix);
                let mask = if prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - prefix)
                };
                (u128::from_be_bytes(network.octets()) & mask)
                    == (u128::from_be_bytes(candidate.octets()) & mask)
            }
            _ => false,
        }
    }
}

/// The only proxy addresses allowed to supply forwarded client-IP headers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TrustedProxySet(Vec<IpNetwork>);

impl TrustedProxySet {
    /// Parses a comma-separated CIDR list. An unset or blank list trusts no
    /// proxy and therefore uses the direct socket peer only.
    pub fn from_spec(spec: Option<&str>) -> Result<Self, MaviError> {
        let Some(spec) = spec else {
            return Ok(Self::default());
        };
        if spec.trim().is_empty() {
            return Ok(Self::default());
        }

        let mut networks = Vec::new();
        for value in spec.split(',') {
            let value = value.trim();
            if value.is_empty() {
                return Err(MaviError::validation("invalid_trusted_proxy_cidr"));
            }
            networks.push(IpNetwork::parse(value)?);
        }
        Ok(Self(networks))
    }

    fn contains(&self, address: IpAddr) -> bool {
        self.0
            .iter()
            .copied()
            .any(|network| network.contains(address))
    }
}

/// HTTP security configuration shared by fixed-site and shard runtimes.
#[derive(Clone, Debug)]
pub struct EdgeSecurityConfig {
    pub trusted_proxies: TrustedProxySet,
    pub policy: EdgeThrottlePolicy,
    limiter: Arc<EdgeRateLimiter>,
}

impl EdgeSecurityConfig {
    pub fn from_trusted_proxy_spec(spec: Option<&str>) -> Result<Self, MaviError> {
        Self::new(
            TrustedProxySet::from_spec(spec)?,
            EdgeThrottlePolicy::default(),
        )
    }

    pub fn new(
        trusted_proxies: TrustedProxySet,
        policy: EdgeThrottlePolicy,
    ) -> Result<Self, MaviError> {
        let limiter = Arc::new(EdgeRateLimiter::new(policy)?);
        Ok(Self {
            trusted_proxies,
            policy,
            limiter,
        })
    }

    pub(crate) fn limiter(&self) -> &EdgeRateLimiter {
        &self.limiter
    }
}

impl Default for EdgeSecurityConfig {
    fn default() -> Self {
        Self::from_trusted_proxy_spec(None).expect("default edge security configuration")
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum SourceKind {
    Ip,
    Device,
}

impl SourceKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Ip => "ip",
            Self::Device => "device",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct BucketKey {
    site_id: SiteId,
    action: EdgeAction,
    kind: SourceKind,
    fingerprint: [u8; 32],
}

#[derive(Clone, Copy, Debug)]
struct Bucket {
    started_at: Instant,
    last_seen: Instant,
    count: u32,
    audit_emitted: bool,
}

impl Bucket {
    const fn new(now: Instant) -> Self {
        Self {
            started_at: now,
            last_seen: now,
            count: 1,
            audit_emitted: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ThrottleScope {
    Ip,
    Device,
}

impl ThrottleScope {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Ip => SourceKind::Ip.as_str(),
            Self::Device => SourceKind::Device.as_str(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ClientSource {
    ip: Option<[u8; 32]>,
    device: Option<[u8; 32]>,
}

impl ClientSource {
    fn signals(self) -> Vec<(SourceKind, [u8; 32])> {
        [(SourceKind::Ip, self.ip), (SourceKind::Device, self.device)]
            .into_iter()
            .filter_map(|(kind, fingerprint)| fingerprint.map(|fingerprint| (kind, fingerprint)))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EdgeDecision {
    pub(crate) limited_scope: Option<ThrottleScope>,
    pub(crate) fingerprint: Option<[u8; 32]>,
    pub(crate) audit_required: bool,
    pub(crate) retry_after_seconds: u64,
}

impl EdgeDecision {
    const fn allowed() -> Self {
        Self {
            limited_scope: None,
            fingerprint: None,
            audit_required: false,
            retry_after_seconds: 0,
        }
    }
}

/// A bounded in-process fixed-window limiter.
///
/// A shard process is the edge boundary for its configured sites. Deployments
/// with multiple edge processes can replace this adapter at the composition
/// root later without changing domain or API code; the key and policy remain
/// site-aware either way.
#[derive(Debug)]
pub struct EdgeRateLimiter {
    policy: EdgeThrottlePolicy,
    buckets: Mutex<HashMap<BucketKey, Bucket>>,
}

impl EdgeRateLimiter {
    pub fn new(policy: EdgeThrottlePolicy) -> Result<Self, MaviError> {
        if policy.ip_limit == 0
            || policy.device_limit == 0
            || policy.ip_window.is_zero()
            || policy.device_window.is_zero()
            || policy.max_buckets < 2
        {
            return Err(MaviError::validation("invalid_edge_throttle_policy"));
        }
        Ok(Self {
            policy,
            buckets: Mutex::new(HashMap::new()),
        })
    }

    pub(crate) fn check(
        &self,
        site_id: SiteId,
        action: EdgeAction,
        source: ClientSource,
        now: Instant,
    ) -> Result<EdgeDecision, MaviError> {
        let sources = source.signals();
        if sources.is_empty() {
            return Ok(EdgeDecision::allowed());
        }

        let max_window = self.policy.ip_window.max(self.policy.device_window);
        let mut buckets = self.buckets.lock().map_err(|_| MaviError::Internal)?;
        buckets.retain(|_, bucket| now.saturating_duration_since(bucket.last_seen) <= max_window);
        if let Some(decision) = self.find_limited(&mut buckets, site_id, action, &sources, now)? {
            return Ok(decision);
        }
        self.record_allowed(&mut buckets, site_id, action, &sources, now)?;
        Ok(EdgeDecision::allowed())
    }

    fn find_limited(
        &self,
        buckets: &mut HashMap<BucketKey, Bucket>,
        site_id: SiteId,
        action: EdgeAction,
        sources: &[(SourceKind, [u8; 32])],
        now: Instant,
    ) -> Result<Option<EdgeDecision>, MaviError> {
        for (kind, fingerprint) in sources {
            let key = BucketKey {
                site_id,
                action,
                kind: *kind,
                fingerprint: *fingerprint,
            };
            let (limit, window) = self.limit_for(*kind);
            let Some(bucket) = buckets.get(&key) else {
                continue;
            };
            let elapsed = now.saturating_duration_since(bucket.started_at);
            if elapsed >= window || bucket.count < limit {
                continue;
            }
            let remaining = window.saturating_sub(elapsed);
            let retry_after_seconds = remaining
                .as_secs()
                .saturating_add(u64::from(remaining.subsec_nanos() != 0))
                .max(1);
            let scope = match kind {
                SourceKind::Ip => ThrottleScope::Ip,
                SourceKind::Device => ThrottleScope::Device,
            };
            let bucket = buckets.get_mut(&key).ok_or(MaviError::Internal)?;
            let audit_required = !bucket.audit_emitted;
            bucket.audit_emitted = true;
            return Ok(Some(EdgeDecision {
                limited_scope: Some(scope),
                fingerprint: Some(*fingerprint),
                audit_required,
                retry_after_seconds,
            }));
        }
        Ok(None)
    }

    fn record_allowed(
        &self,
        buckets: &mut HashMap<BucketKey, Bucket>,
        site_id: SiteId,
        action: EdgeAction,
        sources: &[(SourceKind, [u8; 32])],
        now: Instant,
    ) -> Result<(), MaviError> {
        let missing = sources
            .iter()
            .filter(|(kind, fingerprint)| {
                let key = BucketKey {
                    site_id,
                    action,
                    kind: *kind,
                    fingerprint: *fingerprint,
                };
                let Some(bucket) = buckets.get(&key) else {
                    return true;
                };
                let (_, window) = self.limit_for(*kind);
                now.saturating_duration_since(bucket.started_at) >= window
            })
            .count();
        let max_buckets = self.policy.max_buckets.max(1);
        while buckets.len().saturating_add(missing) > max_buckets {
            let oldest = buckets
                .iter()
                .min_by_key(|(_, bucket)| bucket.last_seen)
                .map(|(key, _)| *key)
                .ok_or(MaviError::Internal)?;
            buckets.remove(&oldest);
        }

        for (kind, fingerprint) in sources {
            let key = BucketKey {
                site_id,
                action,
                kind: *kind,
                fingerprint: *fingerprint,
            };
            let (limit, window) = self.limit_for(*kind);
            match buckets.get_mut(&key) {
                Some(bucket) if now.saturating_duration_since(bucket.started_at) < window => {
                    bucket.count = bucket.count.saturating_add(1).min(limit);
                    bucket.last_seen = now;
                }
                Some(bucket) => *bucket = Bucket::new(now),
                None => {
                    buckets.insert(key, Bucket::new(now));
                }
            }
        }
        Ok(())
    }

    fn limit_for(&self, kind: SourceKind) -> (u32, Duration) {
        match kind {
            SourceKind::Ip => (self.policy.ip_limit, self.policy.ip_window),
            SourceKind::Device => (self.policy.device_limit, self.policy.device_window),
        }
    }
}

pub(crate) fn action_for(request: &Request<Body>) -> Option<EdgeAction> {
    if request.method() != Method::POST {
        return None;
    }
    match request.uri().path() {
        "/api/v1/auth/sessions" => Some(EdgeAction::SessionCreate),
        "/api/v1/auth/password-resets" => Some(EdgeAction::PasswordResetRequest),
        "/api/v1/auth/password-resets/redeem" => Some(EdgeAction::PasswordResetRedeem),
        "/api/v1/auth/email-verifications" => Some(EdgeAction::EmailVerificationRequest),
        "/api/v1/auth/email-verifications/redeem" => Some(EdgeAction::EmailVerificationRedeem),
        _ => None,
    }
}

pub(crate) fn source_for(
    request: &Request<Body>,
    trusted_proxies: &TrustedProxySet,
) -> ClientSource {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect_info| connect_info.0.ip());
    let ip = peer.and_then(|peer| {
        if trusted_proxies.contains(peer) {
            forwarded_ip(request).or(Some(peer))
        } else {
            Some(peer)
        }
    });

    ClientSource {
        ip: ip.map(digest_ip),
        device: request
            .headers()
            .get(USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty() && value.len() <= MAX_USER_AGENT_BYTES)
            .map(digest_device),
    }
}

fn forwarded_ip(request: &Request<Body>) -> Option<IpAddr> {
    request
        .headers()
        .get(FORWARDED)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_forwarded_header)
        .or_else(|| {
            request
                .headers()
                .get(X_FORWARDED_FOR)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(',').next())
                .and_then(parse_ip_token)
        })
}

fn parse_forwarded_header(value: &str) -> Option<IpAddr> {
    value.split(',').next()?.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        name.eq_ignore_ascii_case("for")
            .then(|| parse_ip_token(value.trim()))
            .flatten()
    })
}

fn parse_ip_token(value: &str) -> Option<IpAddr> {
    let value = value.trim().trim_matches('"');
    if value.is_empty() || value.eq_ignore_ascii_case("unknown") || value.starts_with('_') {
        return None;
    }
    if let Some(value) = value.strip_prefix('[') {
        let (address, _) = value.split_once(']')?;
        return address.parse().ok();
    }
    value
        .parse::<IpAddr>()
        .ok()
        .or_else(|| value.parse::<SocketAddr>().ok().map(|address| address.ip()))
}

fn digest_ip(ip: IpAddr) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"mavi-edge-ip-v1\0");
    match ip {
        IpAddr::V4(ip) => hasher.update(ip.octets()),
        IpAddr::V6(ip) => hasher.update(ip.octets()),
    }
    hasher.finalize().into()
}

fn digest_device(user_agent: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"mavi-edge-device-v1\0");
    hasher.update(user_agent.as_bytes());
    hasher.finalize().into()
}

pub(crate) fn fingerprint_text(fingerprint: [u8; 32]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(fingerprint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderValue, Request, header::USER_AGENT};
    use mavi_core::SiteId;

    fn request_with_peer(peer: SocketAddr) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri("/api/v1/auth/sessions")
            .extension(ConnectInfo(peer))
            .body(Body::empty())
            .expect("request")
    }

    #[test]
    fn proxy_spec_supports_ipv4_and_ipv6_networks() {
        let proxies = TrustedProxySet::from_spec(Some("127.0.0.0/8, ::1/128")).expect("proxies");
        assert!(proxies.contains("127.0.0.1".parse().expect("ipv4")));
        assert!(proxies.contains("::1".parse().expect("ipv6")));
        assert!(!proxies.contains("192.0.2.1".parse().expect("other")));
        assert!(TrustedProxySet::from_spec(Some("10.0.0.0/99")).is_err());
    }

    #[test]
    fn forwarded_headers_are_ignored_until_the_peer_is_trusted() {
        let peer = "192.0.2.10:443".parse().expect("peer");
        let mut request = request_with_peer(peer);
        request.headers_mut().insert(
            X_FORWARDED_FOR,
            HeaderValue::from_static("198.51.100.10, 192.0.2.10"),
        );
        let untrusted = TrustedProxySet::default();
        assert_eq!(
            source_for(&request, &untrusted).ip,
            Some(digest_ip(peer.ip()))
        );

        let trusted = TrustedProxySet::from_spec(Some("192.0.2.0/24")).expect("proxy");
        assert_eq!(
            source_for(&request, &trusted).ip,
            Some(digest_ip("198.51.100.10".parse().expect("client")))
        );
    }

    #[test]
    fn forwarded_ipv6_and_device_signals_are_hashed() {
        let peer = "127.0.0.1:443".parse().expect("peer");
        let mut request = request_with_peer(peer);
        request.headers_mut().insert(
            FORWARDED,
            HeaderValue::from_static("for=\"[2001:db8::10]:1234\";proto=https"),
        );
        request
            .headers_mut()
            .insert(USER_AGENT, HeaderValue::from_static("MaviTest/1.0"));
        let trusted = TrustedProxySet::from_spec(Some("127.0.0.1/32")).expect("proxy");
        let source = source_for(&request, &trusted);
        assert_eq!(
            source.ip,
            Some(digest_ip("2001:db8::10".parse().expect("client")))
        );
        assert_eq!(source.device, Some(digest_device("MaviTest/1.0")));
    }

    #[test]
    fn limiter_blocks_after_both_policy_windows_and_audits_once() {
        let policy = EdgeThrottlePolicy {
            ip_limit: 2,
            ip_window: Duration::from_secs(30),
            device_limit: 10,
            device_window: Duration::from_mins(1),
            max_buckets: 16,
        };
        let limiter = EdgeRateLimiter::new(policy).expect("limiter");
        let source = ClientSource {
            ip: Some([1; 32]),
            device: Some([2; 32]),
        };
        let site = SiteId::new();
        let start = Instant::now();
        assert_eq!(
            limiter
                .check(site, EdgeAction::SessionCreate, source, start)
                .expect("first")
                .limited_scope,
            None
        );
        assert_eq!(
            limiter
                .check(
                    site,
                    EdgeAction::SessionCreate,
                    source,
                    start + Duration::from_secs(1),
                )
                .expect("second")
                .limited_scope,
            None
        );
        let blocked = limiter
            .check(
                site,
                EdgeAction::SessionCreate,
                source,
                start + Duration::from_secs(2),
            )
            .expect("limited");
        assert_eq!(blocked.limited_scope, Some(ThrottleScope::Ip));
        assert!(blocked.audit_required);
        assert!(
            !limiter
                .check(
                    site,
                    EdgeAction::SessionCreate,
                    source,
                    start + Duration::from_secs(3),
                )
                .expect("repeated")
                .audit_required
        );
        assert_eq!(
            limiter
                .check(
                    site,
                    EdgeAction::SessionCreate,
                    source,
                    start + Duration::from_secs(31),
                )
                .expect("new window")
                .limited_scope,
            None
        );
    }

    #[test]
    fn device_signal_can_limit_without_an_ip_signal() {
        let limiter = EdgeRateLimiter::new(EdgeThrottlePolicy {
            ip_limit: 100,
            ip_window: Duration::from_mins(1),
            device_limit: 2,
            device_window: Duration::from_mins(10),
            max_buckets: 16,
        })
        .expect("limiter");
        let source = ClientSource {
            ip: None,
            device: Some([3; 32]),
        };
        let site = SiteId::new();
        let start = Instant::now();
        limiter
            .check(site, EdgeAction::PasswordResetRequest, source, start)
            .expect("first");
        limiter
            .check(
                site,
                EdgeAction::PasswordResetRequest,
                source,
                start + Duration::from_secs(1),
            )
            .expect("second");
        let decision = limiter
            .check(
                site,
                EdgeAction::PasswordResetRequest,
                source,
                start + Duration::from_secs(2),
            )
            .expect("limited");
        assert_eq!(decision.limited_scope, Some(ThrottleScope::Device));
    }
}
