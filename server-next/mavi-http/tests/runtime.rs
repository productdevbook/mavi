mod support;

use axum::http::{Method, StatusCode};
use serde_json::Value;
use support::{response_json, send};

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

    let readiness = support::send(&app, Method::GET, "/readyz", None, None).await;
    assert_eq!(readiness.status(), StatusCode::OK);
}
