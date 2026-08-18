use std::{env, sync::Arc};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, header::AUTHORIZATION},
    response::Response,
};
use mavi_core::{SiteContext, SiteId};
use mavi_design::StaticBuildEngine;
use mavi_files::InMemoryFileStore;
use mavi_http::router;
use mavi_runtime::{FixedSiteResolver, Runtime};
use mavi_sealing::KeyringSealer;
use mavi_storage::Database;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

#[allow(dead_code)]
pub async fn build_app() -> Router {
    build_app_with_database().await.0
}

pub async fn build_app_with_database() -> (Router, Database, SiteId) {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");

    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");
    let app = router(
        Runtime::new(database.clone(), FixedSiteResolver::new(site_id)),
        Arc::new(InMemoryFileStore::default()),
        Arc::new(StaticBuildEngine),
        Arc::new(KeyringSealer::from_key([42; 32])),
    )
    .expect("router");
    (app, database, site_id)
}

#[allow(dead_code)]
pub async fn bootstrap(app: &Router, site_name: &str) -> String {
    let setup = send(
        app,
        Method::POST,
        "/api/v1/setup",
        None,
        Some(json!({
            "site_name": site_name,
            "email": "owner@example.com",
            "name": "Owner",
            "password": "long-enough-password"
        })),
    )
    .await;
    assert_eq!(setup.status(), axum::http::StatusCode::CREATED);
    assert!(setup.headers().contains_key("x-request-id"));

    login(app, "owner@example.com").await
}

#[allow(dead_code)]
pub async fn login(app: &Router, email: &str) -> String {
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
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    response_json(response).await["token"]
        .as_str()
        .expect("session token")
        .to_owned()
}

#[allow(dead_code)]
pub async fn verify_email(app: &Router, person: &Value, email: &str) {
    let requested = send(
        app,
        Method::POST,
        "/api/v1/auth/email-verifications",
        None,
        Some(json!({"email": email})),
    )
    .await;
    assert_eq!(requested.status(), axum::http::StatusCode::ACCEPTED);

    let site_id = person["site_id"].as_str().expect("person site id");
    let site_id = SiteId::from_uuid(Uuid::parse_str(site_id).expect("person site id uuid"));
    let database = Database::connect(
        &env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"),
        2,
    )
    .await
    .expect("database connection");
    let context = SiteContext::public(site_id);
    let mut transaction = database.begin(&context).await.expect("mail scope");
    let body: String = sqlx::query_scalar(
        "select body from mail_deliveries
          where site_id = $1 and recipient = $2
          order by created_at desc limit 1",
    )
    .bind(site_id.into_uuid())
    .bind(email)
    .fetch_one(transaction.conn())
    .await
    .expect("verification mail");
    let token = body
        .lines()
        .find(|line| line.starts_with("mavi_verify_"))
        .expect("verification token");
    drop(transaction);

    let redeemed = send(
        app,
        Method::POST,
        "/api/v1/auth/email-verifications/redeem",
        None,
        Some(json!({"token": token})),
    )
    .await;
    assert_eq!(redeemed.status(), axum::http::StatusCode::NO_CONTENT);
}

pub async fn send(
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

#[allow(dead_code)]
pub async fn send_raw(
    app: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    content_type: &str,
    body: impl Into<Body>,
) -> Response {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", content_type);
    if let Some(token) = token {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    app.clone()
        .oneshot(request.body(body.into()).expect("request"))
        .await
        .expect("response")
}

pub async fn response_json(response: Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body");
    serde_json::from_slice(&bytes).expect("json response")
}

#[allow(dead_code)]
pub async fn response_bytes(response: Response) -> Vec<u8> {
    to_bytes(response.into_body(), 10 * 1024 * 1024)
        .await
        .expect("response body")
        .to_vec()
}
