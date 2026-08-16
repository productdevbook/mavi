//! Runtime composition without one router per cloud site.

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use axum::{Router, http::header::HOST, routing::get};
use mavi_core::{MaviError, RequestId, Result, SiteContext, SiteId};
use mavi_storage::{Database, SiteTx};
use uuid::Uuid;

pub type ResolveFuture = Pin<Box<dyn Future<Output = Result<SiteContext>> + Send>>;

pub trait SiteResolver: Send + Sync + 'static {
    fn resolve(&self, headers: axum::http::HeaderMap, request_id: RequestId) -> ResolveFuture;
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
}

/// Cloud edge resolution backed by an allowlisted host-to-site directory.
///
/// The request cannot choose an arbitrary site ID. The edge-facing host must
/// already be present in the resolver's directory, after which the normal
/// request-level `SiteContext` and scoped transaction path is used.
#[derive(Clone, Debug)]
pub struct HostSiteResolver {
    sites: Arc<HashMap<String, SiteId>>,
}

impl HostSiteResolver {
    #[must_use]
    pub fn new(entries: impl IntoIterator<Item = (String, SiteId)>) -> Self {
        let sites = entries
            .into_iter()
            .map(|(host, site_id)| (normalize_host(&host), site_id))
            .collect();
        Self {
            sites: Arc::new(sites),
        }
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
            let site_id = sites.get(&host).copied().ok_or(MaviError::NotFound {
                resource: "site_host",
            })?;
            Ok(SiteContext::with_caller(
                site_id,
                mavi_core::Caller::Public,
                request_id,
            ))
        })
    }
}

fn normalize_host(value: &str) -> String {
    value
        .trim()
        .split_once(':')
        .map_or(value.trim(), |(host, _)| host)
        .to_ascii_lowercase()
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
    }

    #[tokio::test]
    async fn host_resolver_only_accepts_allowlisted_hosts() {
        let site_id = SiteId::new();
        let resolver = HostSiteResolver::new([("Example.com".to_owned(), site_id)]);
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(HOST, "example.com:443".parse().expect("host"));

        let context = resolver
            .resolve(headers, RequestId::new())
            .await
            .expect("known host");
        assert_eq!(context.site_id, site_id);

        let mut unknown_headers = axum::http::HeaderMap::new();
        unknown_headers.insert(HOST, "other.example.com".parse().expect("host"));
        assert!(
            resolver
                .resolve(unknown_headers, RequestId::new())
                .await
                .is_err()
        );
    }

    #[test]
    fn invalid_site_id_is_a_validation_error() {
        assert!(matches!(
            parse_site_id("not-an-id"),
            Err(MaviError::Validation { .. })
        ));
    }
}
