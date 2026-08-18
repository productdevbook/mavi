use axum::{
    Router,
    http::{Method, StatusCode},
};
use mavi_core::{SiteContext, SiteId};
use mavi_storage::Database;
use serde_json::json;

mod support;
use support::{login, protected_mail_body, response_json, send};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn identity_routes_enforce_authz_and_cursor_contracts() {
    let (app, database, site_id) = support::build_app_with_database().await;
    let owner_token = bootstrap(&app).await;
    let reader_token = create_reader(&app, &database, site_id, &owner_token).await;

    assert_cursor_contract(&app, &owner_token).await;
    assert_permission_contract(&app, &reader_token).await;
}

async fn bootstrap(app: &Router) -> String {
    support::bootstrap(app, "HTTP identity test").await
}

async fn create_reader(
    app: &Router,
    database: &Database,
    site_id: SiteId,
    owner_token: &str,
) -> String {
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

    let requested = send(
        app,
        Method::POST,
        "/api/v1/auth/email-verifications",
        None,
        Some(json!({"email": "reader@example.com"})),
    )
    .await;
    assert_eq!(requested.status(), StatusCode::ACCEPTED);

    let body = protected_mail_body(database, site_id, "reader@example.com").await;
    let token = body
        .lines()
        .find(|line| line.starts_with("mavi_verify_"))
        .expect("reader verification token")
        .to_owned();
    let verified = send(
        app,
        Method::POST,
        "/api/v1/auth/email-verifications/redeem",
        None,
        Some(json!({"token": token})),
    )
    .await;
    assert_eq!(verified.status(), StatusCode::NO_CONTENT);

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

    let body = protected_mail_body(&database, site_id, "owner@example.com").await;
    let token = body
        .lines()
        .find(|line| line.starts_with("mavi_reset_"))
        .expect("reset token in provider-neutral mail")
        .to_owned();
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

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn email_verification_response_is_generic_throttled_and_one_time() {
    let (app, database, site_id) = support::build_app_with_database().await;
    let owner_token = support::bootstrap(&app, "HTTP email verification test").await;

    let created = send(
        &app,
        Method::POST,
        "/api/v1/people",
        Some(&owner_token),
        Some(json!({
            "email": "verify@example.com",
            "name": "Verify",
            "password": "long-enough-password"
        })),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(response_json(created).await["email_verified"], false);

    let blocked = send(
        &app,
        Method::POST,
        "/api/v1/auth/sessions",
        None,
        Some(json!({
            "email": "verify@example.com",
            "password": "long-enough-password"
        })),
    )
    .await;
    assert_eq!(blocked.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(blocked).await["error"]["code"],
        "email_not_verified"
    );

    let known = send(
        &app,
        Method::POST,
        "/api/v1/auth/email-verifications",
        None,
        Some(json!({"email": "verify@example.com"})),
    )
    .await;
    let unknown = send(
        &app,
        Method::POST,
        "/api/v1/auth/email-verifications",
        None,
        Some(json!({"email": "missing@example.com"})),
    )
    .await;
    assert_eq!(known.status(), StatusCode::ACCEPTED);
    assert_eq!(unknown.status(), StatusCode::ACCEPTED);
    assert_eq!(response_json(known).await, response_json(unknown).await);

    for _ in 0..4 {
        let response = send(
            &app,
            Method::POST,
            "/api/v1/auth/email-verifications",
            None,
            Some(json!({"email": "verify@example.com"})),
        )
        .await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }
    let limited = send(
        &app,
        Method::POST,
        "/api/v1/auth/email-verifications",
        None,
        Some(json!({"email": "verify@example.com"})),
    )
    .await;
    assert_eq!(limited.status(), StatusCode::ACCEPTED);
    assert_eq!(response_json(limited).await, json!({"accepted": true}));

    let body = protected_mail_body(&database, site_id, "verify@example.com").await;
    let token = body
        .lines()
        .find(|line| line.starts_with("mavi_verify_"))
        .expect("verification token in provider-neutral mail")
        .to_owned();
    let redeemed = send(
        &app,
        Method::POST,
        "/api/v1/auth/email-verifications/redeem",
        None,
        Some(json!({"token": token})),
    )
    .await;
    assert_eq!(redeemed.status(), StatusCode::NO_CONTENT);

    let signed_in = send(
        &app,
        Method::POST,
        "/api/v1/auth/sessions",
        None,
        Some(json!({
            "email": "verify@example.com",
            "password": "long-enough-password"
        })),
    )
    .await;
    assert_eq!(signed_in.status(), StatusCode::CREATED);

    let people = send(
        &app,
        Method::GET,
        "/api/v1/people",
        Some(&owner_token),
        None,
    )
    .await;
    let people = response_json(people).await;
    assert!(
        people["items"]
            .as_array()
            .expect("people")
            .iter()
            .any(|person| {
                person["email"] == "verify@example.com" && person["email_verified"] == true
            })
    );

    let replay = send(
        &app,
        Method::POST,
        "/api/v1/auth/email-verifications/redeem",
        None,
        Some(json!({"token": body.lines().find(|line| line.starts_with("mavi_verify_")).expect("token")})),
    )
    .await;
    assert_eq!(replay.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(replay).await["error"]["code"],
        "email_verification_token_invalid"
    );

    let public_context = SiteContext::public(site_id);
    let mut audit_tx = database.begin(&public_context).await.expect("audit scope");
    let rate_limited: i64 = sqlx::query_scalar(
        "select count(*) from audit_events
          where site_id = $1 and action = 'auth.security.rate_limited'",
    )
    .bind(site_id.into_uuid())
    .fetch_one(audit_tx.conn())
    .await
    .expect("rate limit audit");
    let blocked_login: i64 = sqlx::query_scalar(
        "select count(*) from audit_events
          where site_id = $1 and action = 'auth.session.blocked'",
    )
    .bind(site_id.into_uuid())
    .fetch_one(audit_tx.conn())
    .await
    .expect("blocked login audit");
    assert_eq!(rate_limited, 1);
    assert_eq!(blocked_login, 1);
    audit_tx.commit().await.expect("audit commit");
}
