use axum::http::Method;
use serde_json::json;

mod support;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn credential_http_contract_never_returns_secret_values() {
    let app = support::build_app().await;
    let owner_token = support::bootstrap(&app, "HTTP credentials test").await;

    let created = support::response_json(
        support::send(
            &app,
            Method::POST,
            "/api/v1/credentials",
            Some(&owner_token),
            Some(json!({
                "provider": "mail",
                "name": "primary",
                "values": {"api_key": "do-not-return", "endpoint": "https://mail.example.test"}
            })),
        )
        .await,
    )
    .await;
    assert_eq!(created["provider"], "mail");
    assert_eq!(created["state"], "active");
    assert!(created.get("values").is_none());
    assert!(created.get("sealed_payload").is_none());
    assert!(created.get("password").is_none());

    let id = created["id"].as_str().expect("credential id");
    let listed = support::response_json(
        support::send(
            &app,
            Method::GET,
            "/api/v1/credentials?limit=1",
            Some(&owner_token),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(listed["items"][0]["id"], id);
    assert!(listed["items"][0].get("values").is_none());

    let rotated = support::response_json(
        support::send(
            &app,
            Method::PUT,
            &format!("/api/v1/credentials/{id}"),
            Some(&owner_token),
            Some(json!({
                "expected_version": 1,
                "values": {"api_key": "rotated-secret"}
            })),
        )
        .await,
    )
    .await;
    assert_eq!(rotated["version"], 2);
    assert!(rotated.get("values").is_none());

    let revoked = support::send(
        &app,
        Method::DELETE,
        &format!("/api/v1/credentials/{id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(revoked.status(), axum::http::StatusCode::NO_CONTENT);
}
