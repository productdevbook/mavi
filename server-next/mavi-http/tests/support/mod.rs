use std::net::SocketAddr;
use std::{env, sync::Arc};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::{Method, Request, header::AUTHORIZATION},
    response::Response,
};
use mavi_core::{SiteContext, SiteId, ports::Seals};
use mavi_design::StaticBuildEngine;
use mavi_files::InMemoryFileStore;
use mavi_http::{EdgeSecurityConfig, EdgeThrottlePolicy, TrustedProxySet, router_with_config};
use mavi_runtime::{FixedSiteResolver, HostSiteResolver, Runtime};
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
    build_app_with_edge_policy(EdgeThrottlePolicy::default()).await
}

#[allow(dead_code)]
pub async fn build_shard_app() -> Router {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");

    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");
    let resolver =
        HostSiteResolver::new([(String::from("site.example"), site_id)]).expect("host resolver");
    let edge = EdgeSecurityConfig::new(TrustedProxySet::default(), EdgeThrottlePolicy::default())
        .expect("edge policy");
    router_with_config(
        Runtime::new(database, resolver),
        Arc::new(InMemoryFileStore::default()),
        Arc::new(StaticBuildEngine),
        Arc::new(KeyringSealer::from_key([42; 32])),
        edge,
    )
    .expect("router")
}

#[allow(dead_code)]
pub async fn build_app_with_edge_policy(policy: EdgeThrottlePolicy) -> (Router, Database, SiteId) {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");

    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");
    let edge = EdgeSecurityConfig::new(TrustedProxySet::default(), policy).expect("edge policy");
    let app = router_with_config(
        Runtime::new(database.clone(), FixedSiteResolver::new(site_id)),
        Arc::new(InMemoryFileStore::default()),
        Arc::new(StaticBuildEngine),
        Arc::new(KeyringSealer::from_key([42; 32])),
        edge,
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
pub async fn protected_mail_body(database: &Database, site_id: SiteId, recipient: &str) -> String {
    let context = SiteContext::public(site_id);
    let mut transaction = database.begin(&context).await.expect("mail scope");
    let (stored_body, body_protected, ciphertext): (String, bool, Vec<u8>) = sqlx::query_as(
        "select d.body, d.body_protected, s.ciphertext
           from mail_delivery_secrets s
           join mail_deliveries d on d.site_id = s.site_id and d.id = s.delivery_id
          where d.site_id = $1 and d.recipient = $2 and d.body_protected
          order by d.created_at desc limit 1",
    )
    .bind(site_id.into_uuid())
    .bind(recipient)
    .fetch_one(transaction.conn())
    .await
    .expect("protected mail secret");
    transaction.commit().await.expect("mail commit");
    assert_eq!(stored_body, "[protected]");
    assert!(body_protected);

    let plaintext = KeyringSealer::from_key([42; 32])
        .unseal(&context, &ciphertext)
        .await
        .expect("protected mail unseal");
    String::from_utf8(plaintext).expect("protected mail utf8")
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
    let body = protected_mail_body(&database, site_id, email).await;
    let token = body
        .lines()
        .find(|line| line.starts_with("mavi_verify_"))
        .expect("verification token");

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
pub async fn send_with_peer(
    app: &Router,
    method: Method,
    uri: &str,
    peer: SocketAddr,
    payload: Option<Value>,
) -> Response {
    let mut request = Request::builder().method(method).uri(uri);
    if payload.is_some() {
        request = request.header("content-type", "application/json");
    }
    let body = payload.map_or_else(Body::empty, |payload| Body::from(payload.to_string()));
    let mut request = request.body(body).expect("request");
    request.extensions_mut().insert(ConnectInfo(peer));
    app.clone().oneshot(request).await.expect("response")
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
