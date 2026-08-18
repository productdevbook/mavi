mod support;

use axum::http::{Method, StatusCode};
use mavi_http::api;
use serde_json::json;
use support::{bootstrap, response_json, send};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn portable_http_export_and_cross_site_import_are_explicit() {
    let source_app = support::build_app().await;
    let source_token = bootstrap(&source_app, "Portable source").await;
    let canonical = send(
        &source_app,
        Method::PATCH,
        "/api/v1/settings",
        Some(&source_token),
        Some(json!({"canonical_url": "https://portable.example.test/site/"})),
    )
    .await;
    assert_eq!(canonical.status(), StatusCode::OK);
    let content = send(
        &source_app,
        Method::POST,
        "/api/v1/content",
        Some(&source_token),
        Some(json!({
            "kind": "post",
            "language": "en",
            "slug": "portable-post",
            "title": "Portable post",
            "excerpt": null,
            "body": "Export me",
            "fields": {},
            "publication": "draft"
        })),
    )
    .await;
    assert_eq!(content.status(), StatusCode::CREATED);

    let source_export = send(
        &source_app,
        Method::GET,
        "/api/v1/portable/export",
        Some(&source_token),
        None,
    )
    .await;
    assert_eq!(source_export.status(), StatusCode::OK);
    let bundle = response_json(source_export).await;
    assert_eq!(bundle["manifest"]["format"], "mavi.portable");
    assert_eq!(bundle["manifest"]["version"], 2);
    assert_eq!(
        bundle["site"]["canonical_url"],
        "https://portable.example.test/site"
    );
    assert!(bundle.get("offset").is_none());

    let target_app = support::build_app().await;
    let target_token = bootstrap(&target_app, "Portable target").await;
    let imported = send(
        &target_app,
        Method::POST,
        "/api/v1/portable/import",
        Some(&target_token),
        Some(json!({"bundle": bundle, "strategy": "upsert"})),
    )
    .await;
    assert_eq!(imported.status(), StatusCode::OK);
    assert_eq!(response_json(imported).await["content"], 1);

    let imported_settings = send(
        &target_app,
        Method::GET,
        "/api/v1/settings",
        Some(&target_token),
        None,
    )
    .await;
    assert_eq!(imported_settings.status(), StatusCode::OK);
    assert_eq!(
        response_json(imported_settings).await["canonical_url"],
        "https://portable.example.test/site"
    );

    let listed = send(
        &target_app,
        Method::GET,
        "/api/v1/content?limit=1",
        Some(&target_token),
        None,
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(
        response_json(listed).await["items"][0]["slug"],
        "portable-post"
    );

    let anonymous = send(
        &target_app,
        Method::GET,
        "/api/v1/portable/export",
        None,
        None,
    )
    .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let catalog = api();
    assert!(
        catalog
            .endpoints
            .iter()
            .any(|endpoint| endpoint.operation_id == "portable.export")
    );
    assert!(
        catalog
            .endpoints
            .iter()
            .any(|endpoint| endpoint.operation_id == "portable.import")
    );
}
