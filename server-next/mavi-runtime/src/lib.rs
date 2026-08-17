//! Runtime composition without one router per cloud site.

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, RwLock},
};

use axum::{Router, http::header::HOST, routing::get};
use mavi_core::{MaviError, RequestId, Result, SiteContext, SiteId};
use mavi_storage::{CURRENT_SCHEMA_VERSION, Database, SiteTx};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const RUNTIME_PROTOCOL: &str = "mavi.runtime.v1";
pub const API_CONTRACT_VERSION: &str = "v1";
pub const PAGINATION_STYLE: &str = "cursor";
pub const MAX_PAGE_LIMIT: u16 = 100;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    FixedSite,
    Shard,
}

impl RuntimeMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FixedSite => "fixed_site",
            Self::Shard => "shard",
        }
    }
}

/// The machine-readable compatibility contract consumed by a panel or
/// operator after a site is provisioned.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimeManifest {
    pub protocol: String,
    pub release: String,
    pub api_contract_version: String,
    pub api_contract_hash: String,
    pub storage_schema_version: u32,
    pub runtime_mode: RuntimeMode,
    pub site_id: SiteId,
    pub pagination: PaginationContract,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PaginationContract {
    pub style: String,
    pub default_limit: u16,
    pub max_limit: u16,
}

impl RuntimeManifest {
    #[must_use]
    pub fn new(site_id: SiteId, runtime_mode: RuntimeMode, api_contract_hash: String) -> Self {
        Self {
            protocol: RUNTIME_PROTOCOL.to_owned(),
            release: env!("CARGO_PKG_VERSION").to_owned(),
            api_contract_version: API_CONTRACT_VERSION.to_owned(),
            api_contract_hash,
            storage_schema_version: CURRENT_SCHEMA_VERSION,
            runtime_mode,
            site_id,
            pagination: PaginationContract {
                style: PAGINATION_STYLE.to_owned(),
                default_limit: mavi_core::PageRequest::DEFAULT_LIMIT,
                max_limit: MAX_PAGE_LIMIT,
            },
        }
    }
}

pub type ResolveFuture = Pin<Box<dyn Future<Output = Result<SiteContext>> + Send>>;

pub trait SiteResolver: Send + Sync + 'static {
    fn resolve(&self, headers: axum::http::HeaderMap, request_id: RequestId) -> ResolveFuture;

    fn mode(&self) -> RuntimeMode;
}

#[derive(Clone, Debug)]
pub struct FixedSiteResolver {
    site_id: SiteId,
}

impl FixedSiteResolver {
    #[must_use]
    pub const fn new(site_id: SiteId) -> Self {
        Self { site_id }
    }
}

impl SiteResolver for FixedSiteResolver {
    fn resolve(&self, _headers: axum::http::HeaderMap, request_id: RequestId) -> ResolveFuture {
        let site_id = self.site_id;
        Box::pin(async move {
            Ok(SiteContext::with_caller(
                site_id,
                mavi_core::Caller::Public,
                request_id,
            ))
        })
    }

    fn mode(&self) -> RuntimeMode {
        RuntimeMode::FixedSite
    }
}

/// Cloud edge resolution backed by an allowlisted host-to-site directory.
///
/// The request cannot choose an arbitrary site ID. The edge-facing host must
/// already be present in the resolver's directory, after which the normal
/// request-level `SiteContext` and scoped transaction path is used.
#[derive(Clone, Debug)]
pub struct HostSiteResolver {
    sites: Arc<RwLock<HashMap<String, SiteId>>>,
}

impl HostSiteResolver {
    pub fn new(entries: impl IntoIterator<Item = (String, SiteId)>) -> Result<Self> {
        Ok(Self {
            sites: Arc::new(RwLock::new(checked_entries(entries)?)),
        })
    }

    /// Replaces the host directory as one in-memory snapshot.
    ///
    /// A resolver never observes a half-written directory. Duplicate hosts
    /// claimed by different sites are refused before the old snapshot is
    /// replaced, so a bad control-plane refresh cannot redirect traffic.
    pub fn replace(&self, entries: impl IntoIterator<Item = (String, SiteId)>) -> Result<()> {
        let next = checked_entries(entries)?;
        *self.sites.write().map_err(|_| MaviError::Internal)? = next;
        Ok(())
    }
}

impl SiteResolver for HostSiteResolver {
    fn resolve(&self, headers: axum::http::HeaderMap, request_id: RequestId) -> ResolveFuture {
        let sites = Arc::clone(&self.sites);
        Box::pin(async move {
            let host = headers
                .get(HOST)
                .and_then(|value| value.to_str().ok())
                .map(normalize_host)
                .ok_or_else(|| MaviError::validation("site_host_required"))?;
            let site_id = sites
                .read()
                .map_err(|_| MaviError::Internal)?
                .get(&host)
                .copied()
                .ok_or(MaviError::NotFound {
                    resource: "site_host",
                })?;
            Ok(SiteContext::with_caller(
                site_id,
                mavi_core::Caller::Public,
                request_id,
            ))
        })
    }

    fn mode(&self) -> RuntimeMode {
        RuntimeMode::Shard
    }
}

fn normalize_host(value: &str) -> String {
    value
        .trim()
        .split_once(':')
        .map_or(value.trim(), |(host, _)| host)
        .to_ascii_lowercase()
}

fn checked_entries(
    entries: impl IntoIterator<Item = (String, SiteId)>,
) -> Result<HashMap<String, SiteId>> {
    let mut sites = HashMap::new();

    for (raw_host, site_id) in entries {
        let host = normalize_host(&raw_host);
        if host.is_empty() {
            return Err(MaviError::validation("site_host_required"));
        }

        if let Some(previous) = sites.insert(host, site_id)
            && previous != site_id
        {
            return Err(MaviError::validation("duplicate_site_host"));
        }
    }

    Ok(sites)
}

/// One router and one pool can serve many site scopes in a shard.
#[derive(Debug)]
pub struct Runtime<R> {
    database: Database,
    resolver: Arc<R>,
}

impl<R> Clone for Runtime<R> {
    fn clone(&self) -> Self {
        Self {
            database: self.database.clone(),
            resolver: Arc::clone(&self.resolver),
        }
    }
}

impl<R> Runtime<R>
where
    R: SiteResolver,
{
    #[must_use]
    pub fn new(database: Database, resolver: R) -> Self {
        Self {
            database,
            resolver: Arc::new(resolver),
        }
    }

    pub async fn context(
        &self,
        headers: axum::http::HeaderMap,
        request_id: RequestId,
    ) -> Result<SiteContext> {
        self.resolver.resolve(headers, request_id).await
    }

    pub async fn begin(&self, context: &SiteContext) -> Result<SiteTx> {
        self.database.begin(context).await
    }

    #[must_use]
    pub fn mode(&self) -> RuntimeMode {
        self.resolver.mode()
    }

    #[must_use]
    pub fn manifest(&self, site_id: SiteId, api_contract_hash: String) -> RuntimeManifest {
        RuntimeManifest::new(site_id, self.mode(), api_contract_hash)
    }

    pub fn router<S>(&self) -> Router<S>
    where
        S: Clone + Send + Sync + 'static,
    {
        Router::new()
            .route("/healthz", get(health))
            .route("/api/v1/health", get(health))
    }
}

async fn health() -> &'static str {
    "ok"
}

pub fn parse_site_id(value: &str) -> Result<SiteId> {
    Uuid::parse_str(value)
        .map(SiteId::from_uuid)
        .map_err(|_| MaviError::validation("invalid_site_id"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fixed_runtime_always_resolves_the_configured_site() {
        let site_id = SiteId::new();
        let resolver = FixedSiteResolver::new(site_id);
        let context = resolver
            .resolve(axum::http::HeaderMap::new(), RequestId::new())
            .await
            .expect("fixed resolver should resolve");

        assert_eq!(context.site_id, site_id);
        assert!(context.caller.is_public());
        assert_eq!(resolver.mode(), RuntimeMode::FixedSite);
    }

    #[tokio::test]
    async fn host_resolver_only_accepts_allowlisted_hosts() {
        let site_id = SiteId::new();
        let resolver = HostSiteResolver::new([("Example.com".to_owned(), site_id)])
            .expect("valid host directory");
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(HOST, "example.com:443".parse().expect("host"));

        let context = resolver
            .resolve(headers, RequestId::new())
            .await
            .expect("known host");
        assert_eq!(context.site_id, site_id);
        assert_eq!(resolver.mode(), RuntimeMode::Shard);

        let mut unknown_headers = axum::http::HeaderMap::new();
        unknown_headers.insert(HOST, "other.example.com".parse().expect("host"));
        assert!(
            resolver
                .resolve(unknown_headers, RequestId::new())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn host_resolver_replaces_its_directory_as_one_snapshot() {
        let first = SiteId::new();
        let second = SiteId::new();
        let resolver = HostSiteResolver::new([("first.example.com".to_owned(), first)])
            .expect("valid host directory");

        resolver
            .replace([("second.example.com".to_owned(), second)])
            .expect("valid replacement");

        let mut headers = axum::http::HeaderMap::new();
        headers.insert(HOST, "second.example.com".parse().expect("host"));
        assert_eq!(
            resolver
                .resolve(headers, RequestId::new())
                .await
                .unwrap()
                .site_id,
            second
        );

        let duplicate = resolver.replace([
            ("same.example.com".to_owned(), first),
            ("SAME.EXAMPLE.COM:443".to_owned(), second),
        ]);
        assert!(duplicate.is_err());
    }

    #[test]
    fn invalid_site_id_is_a_validation_error() {
        assert!(matches!(
            parse_site_id("not-an-id"),
            Err(MaviError::Validation { .. })
        ));
    }

    #[test]
    fn manifest_is_explicit_about_release_scope_and_cursor_pagination() {
        let site_id = SiteId::new();
        let manifest = RuntimeManifest::new(
            site_id,
            RuntimeMode::FixedSite,
            "sha256:contract".to_owned(),
        );

        assert_eq!(manifest.protocol, RUNTIME_PROTOCOL);
        assert_eq!(manifest.site_id, site_id);
        assert_eq!(manifest.pagination.style, PAGINATION_STYLE);
        assert_eq!(manifest.pagination.max_limit, MAX_PAGE_LIMIT);
        assert_eq!(manifest.storage_schema_version, CURRENT_SCHEMA_VERSION);
    }
}
