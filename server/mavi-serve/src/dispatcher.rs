//! Multi-site request dispatcher.
//!
//! Routes incoming HTTP requests to independent site routers based on the `Host`
//! header (e.g. `domain.com`, `tenant.example.com`, or fallback).
//!
//! Each site is a distinct, self-contained router with its own isolated database
//! connection, storage, keyring, and state — enabling multiple sites in one process
//! without single-site schema overhead or global state leakage (Issue #134).

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::header::HOST;
use axum::response::{IntoResponse, Response};
use tower::ServiceExt;

use crate::refusal;

/// Dispatches requests to different site routers based on hostname.
#[derive(Clone, Default)]
pub struct HostDispatcher {
    routes: Arc<HashMap<String, Router>>,
    fallback: Option<Router>,
}

impl std::fmt::Debug for HostDispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostDispatcher")
            .field("hosts", &self.routes.keys().collect::<Vec<_>>())
            .field("has_fallback", &self.fallback.is_some())
            .finish()
    }
}

impl HostDispatcher {
    /// Creates a new empty host dispatcher.
    #[must_use]
    pub fn new() -> Self {
        Self {
            routes: Arc::new(HashMap::new()),
            fallback: None,
        }
    }

    /// Associates a hostname (e.g., `"site1.example.com"` or `"localhost"`) with a site router.
    ///
    /// Hostnames are normalized to lowercase and stripped of any port numbers.
    #[must_use]
    pub fn with_site(mut self, host: impl Into<String>, site_router: Router) -> Self {
        let normalized = normalize_host(&host.into());
        let mut map = (*self.routes).clone();
        map.insert(normalized, site_router);
        self.routes = Arc::new(map);
        self
    }

    /// Sets the fallback router for requests that do not match any registered host.
    #[must_use]
    pub fn with_fallback(mut self, fallback_router: Router) -> Self {
        self.fallback = Some(fallback_router);
        self
    }

    /// Dispatches a single incoming request to the matching site router or fallback.
    pub async fn dispatch(&self, req: Request<Body>) -> Response {
        let host = extract_host(&req);

        let target = host
            .as_deref()
            .and_then(|h| self.routes.get(h))
            .or(self.fallback.as_ref());

        if let Some(target_router) = target {
            let mut matched = target_router.clone();
            match matched.as_service().oneshot(req).await {
                Ok(res) => res.into_response(),
                Err(err) => match err {},
            }
        } else {
            tracing::warn!(host = ?host, "no site registered for host and no fallback set");
            refusal::nothing_answers_there()
        }
    }

    /// Converts this dispatcher into an `axum::Router`.
    pub fn into_router(self) -> Router {
        let dispatcher = Arc::new(self);
        Router::new().fallback(move |req: Request<Body>| {
            let dispatcher = Arc::clone(&dispatcher);
            async move { dispatcher.dispatch(req).await }
        })
    }

    /// Returns the list of registered hostnames.
    #[must_use]
    pub fn hosts(&self) -> Vec<String> {
        let mut list: Vec<String> = self.routes.keys().cloned().collect();
        list.sort();
        list
    }
}

/// Normalizes a host string by trimming, lowercasing, and stripping any port component.
fn normalize_host(host: &str) -> String {
    let trimmed = host.trim().to_lowercase();
    trimmed.split(':').next().unwrap_or(&trimmed).to_owned()
}

/// Extracts and normalizes the host from the request headers.
fn extract_host(req: &Request<Body>) -> Option<String> {
    req.headers()
        .get(HOST)
        .and_then(|h| h.to_str().ok())
        .map(normalize_host)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::Request;
    use axum::routing::get;
    use tower::ServiceExt;

    #[tokio::test]
    async fn dispatches_to_correct_site_by_host() {
        let site_a = Router::new().route("/hello", get(|| async { "Site A" }));
        let site_b = Router::new().route("/hello", get(|| async { "Site B" }));
        let fallback = Router::new().route("/hello", get(|| async { "Fallback" }));

        let dispatcher = HostDispatcher::new()
            .with_site("sitea.example.com", site_a)
            .with_site("siteb.example.com:8080", site_b)
            .with_fallback(fallback)
            .into_router();

        // Request to Site A
        let request_a = Request::builder()
            .uri("/hello")
            .header("Host", "sitea.example.com")
            .body(Body::empty())
            .unwrap();
        let response_a = dispatcher.clone().oneshot(request_a).await.unwrap();
        assert_eq!(response_a.status(), axum::http::StatusCode::OK);
        let body_a = to_bytes(response_a.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body_a[..], b"Site A");

        // Request to Site B (with port)
        let request_b = Request::builder()
            .uri("/hello")
            .header("Host", "siteb.example.com:8080")
            .body(Body::empty())
            .unwrap();
        let response_b = dispatcher.clone().oneshot(request_b).await.unwrap();
        assert_eq!(response_b.status(), axum::http::StatusCode::OK);
        let body_b = to_bytes(response_b.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body_b[..], b"Site B");

        // Request to unknown host falls back
        let request_fallback = Request::builder()
            .uri("/hello")
            .header("Host", "unknown.example.com")
            .body(Body::empty())
            .unwrap();
        let response_fallback = dispatcher.clone().oneshot(request_fallback).await.unwrap();
        assert_eq!(response_fallback.status(), axum::http::StatusCode::OK);
        let body_fallback = to_bytes(response_fallback.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body_fallback[..], b"Fallback");
    }

    #[tokio::test]
    async fn returns_refusal_when_no_match_and_no_fallback() {
        let site_a = Router::new().route("/hello", get(|| async { "Site A" }));
        let dispatcher = HostDispatcher::new()
            .with_site("sitea.example.com", site_a)
            .into_router();

        let req = Request::builder()
            .uri("/hello")
            .header("Host", "other.example.com")
            .body(Body::empty())
            .unwrap();
        let res = dispatcher.oneshot(req).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::NOT_FOUND);
    }
}
