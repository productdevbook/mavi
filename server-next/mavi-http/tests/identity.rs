use std::env;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header::AUTHORIZATION},
    response::Response,
};
use mavi_core::SiteId;
use mavi_http::router;
use mavi_runtime::{FixedSiteResolver, Runtime};
use mavi_storage::Database;
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn identity_routes_enforce_authz_and_cursor_contracts() {
    let app = build_app().await;
    let owner_token = bootstrap(&app).await;
    let reader_token = create_reader(&app, &owner_token).await;

    assert_cursor_contract(&app, &owner_token).await;
    assert_permission_contract(&app, &reader_token).await;
}

async fn build_app() -> Router {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");

    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");
    router(Runtime::new(database, FixedSiteResolver::new(site_id))).expect("router")
}

async fn bootstrap(app: &Router) -> String {
    let setup = send(
        app,
        Method::POST,
        "/api/v1/setup",
        None,
        Some(json!({
            "site_name": "HTTP identity test",
            "email": "owner@example.com",
            "name": "Owner",
            "password": "long-enough-password"
        })),
    )
    .await;
    assert_eq!(setup.status(), StatusCode::CREATED);
    assert!(setup.headers().contains_key("x-request-id"));

    login(app, "owner@example.com").await
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
    if first_page.status() != StatusCode::OK {
        let body = response_text(first_page).await;
        panic!("people list failed: {body}");
    }
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

async fn login(app: &Router, email: &str) -> String {
    let response = send(
        app,
        Method::POST,
        "/api/v1/auth/sessions",
        None,
        Some(json!({
            "email": email,
            "password": "long-enough-password"
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["token"]
        .as_str()
        .expect("session token")
        .to_owned()
}

async fn send(
    app: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    payload: Option<Value>,
) -> Response {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    if payload.is_some() {
        request = request.header("content-type", "application/json");
    }
    let body = payload.map_or_else(Body::empty, |payload| Body::from(payload.to_string()));
    app.clone()
        .oneshot(request.body(body).expect("request"))
        .await
        .expect("response")
}

async fn response_json(response: Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body");
    serde_json::from_slice(&bytes).expect("json response")
}

async fn response_text(response: Response) -> String {
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body");
    String::from_utf8_lossy(&bytes).into_owned()
}
