mod support;

use axum::http::{Method, StatusCode};
use serde_json::json;
use support::{build_app_with_database, response_json, send};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a PostgreSQL role that is subject to RLS"]
async fn write_fence_blocks_mutations_but_keeps_reads_available() {
    let (app, database, site_id) = build_app_with_database().await;
    let token = support::bootstrap(&app, "Fence site").await;
    let fence_token = uuid::Uuid::now_v7();

    database
        .acquire_write_fence(site_id, fence_token, "relocation-final-copy")
        .await
        .expect("acquire fence");

    let write = send(
        &app,
        Method::POST,
        "/api/v1/auth/sessions",
        None,
        Some(json!({
            "email": "owner@example.com",
            "password": "long-enough-password"
        })),
    )
    .await;
    assert_eq!(write.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(write).await["error"]["code"],
        "site_write_fenced"
    );

    let read = send(&app, Method::GET, "/api/v1/setup", Some(&token), None).await;
    assert_eq!(read.status(), StatusCode::OK);

    database
        .release_write_fence(site_id, fence_token)
        .await
        .expect("release fence");
    let writable = send(
        &app,
        Method::POST,
        "/api/v1/auth/sessions",
        None,
        Some(json!({
            "email": "owner@example.com",
            "password": "long-enough-password"
        })),
    )
    .await;
    assert_eq!(writable.status(), StatusCode::CREATED);
}
