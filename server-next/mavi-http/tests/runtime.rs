mod support;

use axum::{
    body::to_bytes,
    http::{Method, StatusCode},
};
use serde_json::Value;
use support::{response_json, send};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn runtime_manifest_is_public_site_scoped_and_cursor_only() {
    let app = support::build_app().await;
    let response = send(&app, Method::GET, "/api/v1/runtime/manifest", None, None).await;

    assert_eq!(response.status(), StatusCode::OK);
    let manifest: Value = response_json(response).await;
    assert_eq!(manifest["protocol"], "mavi.runtime.v1");
    assert_eq!(manifest["api_contract_version"], "v1");
    assert!(
        manifest["api_contract_hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:") && hash.len() == 71)
    );
    assert_eq!(manifest["pagination"]["style"], "cursor");
    assert_eq!(manifest["pagination"]["max_limit"], 100);
    assert!(manifest.get("page").is_none());
    assert!(manifest.get("offset").is_none());
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn liveness_and_readiness_are_global_in_shard_mode() {
    let app = support::build_shard_app().await;

    let liveness = support::send(&app, Method::GET, "/healthz", None, None).await;
    assert_eq!(liveness.status(), StatusCode::OK);
    let liveness_request_id = liveness
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("liveness request id");
    assert!(Uuid::parse_str(liveness_request_id).is_ok());

    let readiness = support::send(&app, Method::GET, "/readyz", None, None).await;
    assert_eq!(readiness.status(), StatusCode::OK);
    let readiness_request_id = readiness
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("readiness request id");
    assert!(Uuid::parse_str(readiness_request_id).is_ok());
    assert_ne!(liveness_request_id, readiness_request_id);

    let metrics = support::send(&app, Method::GET, "/metrics", None, None).await;
    assert_eq!(metrics.status(), StatusCode::OK);
    assert_eq!(
        metrics
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("text/plain; version=0.0.4; charset=utf-8")
    );
    let metrics_body = to_bytes(metrics.into_body(), usize::MAX)
        .await
        .expect("metrics body");
    let metrics_body = String::from_utf8(metrics_body.to_vec()).expect("metrics text");
    assert!(metrics_body.contains("mavi_http_requests_total"));
    assert!(metrics_body.contains("mavi_worker_polls_total"));
}
