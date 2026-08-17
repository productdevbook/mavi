use axum::{
    Router,
    http::{Method, StatusCode},
};
use serde_json::json;

mod support;
use support::{build_app, login, response_json, send};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn identity_routes_enforce_authz_and_cursor_contracts() {
    let app = build_app().await;
    let owner_token = bootstrap(&app).await;
    let reader_token = create_reader(&app, &owner_token).await;

    assert_cursor_contract(&app, &owner_token).await;
    assert_permission_contract(&app, &reader_token).await;
}

async fn bootstrap(app: &Router) -> String {
    support::bootstrap(app, "HTTP identity test").await
}

async fn create_reader(app: &Router, owner_token: &str) -> String {
    let role = send(
        app,
        Method::POST,
        "/api/v1/roles",
        Some(owner_token),
        Some(json!({
            "name": "reader",
            "grants": [{"capability": "content", "action": "view"}]
        })),
    )
    .await;
    assert_eq!(role.status(), StatusCode::CREATED);
    let role_id = response_json(role).await["id"]
        .as_str()
        .expect("role id")
        .to_owned();

    let person = send(
        app,
        Method::POST,
        "/api/v1/people",
        Some(owner_token),
        Some(json!({
            "email": "reader@example.com",
            "name": "Reader",
            "password": "long-enough-password",
            "role_ids": [role_id]
        })),
    )
    .await;
    assert_eq!(person.status(), StatusCode::CREATED);

    login(app, "reader@example.com").await
}

async fn assert_cursor_contract(app: &Router, owner_token: &str) {
    let first_page = send(
        app,
        Method::GET,
        "/api/v1/people?limit=1",
        Some(owner_token),
        None,
    )
    .await;
    assert_eq!(first_page.status(), StatusCode::OK);
    let first_page = response_json(first_page).await;
    let cursor = first_page["next_cursor"].as_str().expect("next cursor");
    let second_page_uri = format!("/api/v1/people?limit=1&after={cursor}");
    let second_page = send(app, Method::GET, &second_page_uri, Some(owner_token), None).await;
    assert_eq!(second_page.status(), StatusCode::OK);

    let invalid_cursor = send(
        app,
        Method::GET,
        "/api/v1/people?after=not-a-valid-cursor",
        Some(owner_token),
        None,
    )
    .await;
    assert_eq!(invalid_cursor.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(invalid_cursor).await["error"]["code"],
        "invalid_cursor"
    );

    let unauthenticated = send(app, Method::GET, "/api/v1/people", None, None).await;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
}

async fn assert_permission_contract(app: &Router, reader_token: &str) {
    let forbidden = send(app, Method::GET, "/api/v1/people", Some(reader_token), None).await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert_eq!(response_json(forbidden).await["error"]["code"], "forbidden");

    let content_list = send(
        app,
        Method::GET,
        "/api/v1/content?limit=1",
        Some(reader_token),
        None,
    )
    .await;
    assert_eq!(content_list.status(), StatusCode::OK);
}
