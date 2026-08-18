use axum::{
    Router,
    http::{Method, StatusCode},
};
use mavi_core::SiteContext;
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

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn password_reset_response_is_generic_and_redeems_the_mail_outbox_token() {
    let (app, database, site_id) = support::build_app_with_database().await;
    support::bootstrap(&app, "HTTP password reset test").await;

    let known = send(
        &app,
        Method::POST,
        "/api/v1/auth/password-resets",
        None,
        Some(json!({"email": "owner@example.com"})),
    )
    .await;
    let unknown = send(
        &app,
        Method::POST,
        "/api/v1/auth/password-resets",
        None,
        Some(json!({"email": "missing@example.com"})),
    )
    .await;
    assert_eq!(known.status(), StatusCode::ACCEPTED);
    assert_eq!(unknown.status(), StatusCode::ACCEPTED);
    assert_eq!(response_json(known).await, response_json(unknown).await);

    let public_context = SiteContext::public(site_id);
    let mut tx = database.begin(&public_context).await.expect("mail scope");
    let body: String = sqlx::query_scalar(
        "select body from mail_deliveries where site_id = $1 order by created_at desc limit 1",
    )
    .bind(site_id.into_uuid())
    .fetch_one(tx.conn())
    .await
    .expect("password reset mail");
    let token = body
        .lines()
        .find(|line| line.starts_with("mavi_reset_"))
        .expect("reset token in provider-neutral mail")
        .to_owned();
    tx.commit().await.expect("mail commit");

    let redeemed = send(
        &app,
        Method::POST,
        "/api/v1/auth/password-resets/redeem",
        None,
        Some(json!({
            "token": token,
            "password": "new-long-enough-password"
        })),
    )
    .await;
    assert_eq!(redeemed.status(), StatusCode::NO_CONTENT);

    let old_password = send(
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
    assert_eq!(old_password.status(), StatusCode::UNAUTHORIZED);
    let new_password = send(
        &app,
        Method::POST,
        "/api/v1/auth/sessions",
        None,
        Some(json!({
            "email": "owner@example.com",
            "password": "new-long-enough-password"
        })),
    )
    .await;
    assert_eq!(new_password.status(), StatusCode::CREATED);
}
