use axum::{
    Router,
    http::{Method, StatusCode},
};
use mavi_core::{SiteContext, SiteId};
use mavi_http::EdgeThrottlePolicy;
use mavi_storage::Database;
use serde_json::json;
use std::time::Duration;

mod support;
use support::{login, protected_mail_body, response_json, send, send_with_peer};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn current_session_returns_only_the_authenticated_person_grants() {
    let (app, _database, _site_id) = support::build_app_with_database().await;
    let owner_token = bootstrap(&app).await;

    let anonymous = send(
        &app,
        Method::GET,
        "/api/v1/auth/sessions/current",
        None,
        None,
    )
    .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let current = send(
        &app,
        Method::GET,
        "/api/v1/auth/sessions/current",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(current.status(), StatusCode::OK);
    let current = response_json(current).await;
    assert_eq!(current["person"]["email"], "owner@example.com");
    assert_eq!(current["person"]["name"], "Owner");
    assert!(current["grants"].as_array().is_some_and(|grants| {
        grants
            .iter()
            .any(|grant| grant["capability"] == "content" && grant["action"] == "view")
    }));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn identity_routes_enforce_authz_and_cursor_contracts() {
    let (app, database, site_id) = support::build_app_with_database().await;
    let owner_token = bootstrap(&app).await;
    let reader_token = create_reader(&app, &database, site_id, &owner_token).await;

    assert_cursor_contract(&app, &owner_token).await;
    assert_permission_contract(&app, &reader_token).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn role_lifecycle_protects_owner_and_assigned_roles() {
    let (app, _database, _site_id) = support::build_app_with_database().await;
    let owner_token = bootstrap(&app).await;

    let roles = send(&app, Method::GET, "/api/v1/roles", Some(&owner_token), None).await;
    assert_eq!(roles.status(), StatusCode::OK);
    let owner_role_id = response_json(roles).await["items"]
        .as_array()
        .expect("role items")
        .iter()
        .find(|role| role["name"] == "owner")
        .and_then(|role| role["id"].as_str())
        .expect("owner role id")
        .to_owned();

    let unused = send(
        &app,
        Method::POST,
        "/api/v1/roles",
        Some(&owner_token),
        Some(json!({
            "name": "unused",
            "grants": [{"capability": "content", "action": "view"}]
        })),
    )
    .await;
    assert_eq!(unused.status(), StatusCode::CREATED);
    let unused_id = response_json(unused).await["id"]
        .as_str()
        .expect("unused role id")
        .to_owned();

    let deleted = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/roles/{unused_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let assigned = send(
        &app,
        Method::POST,
        "/api/v1/roles",
        Some(&owner_token),
        Some(json!({
            "name": "assigned",
            "grants": [{"capability": "content", "action": "view"}]
        })),
    )
    .await;
    assert_eq!(assigned.status(), StatusCode::CREATED);
    let assigned_id = response_json(assigned).await["id"]
        .as_str()
        .expect("assigned role id")
        .to_owned();
    let person = send(
        &app,
        Method::POST,
        "/api/v1/people",
        Some(&owner_token),
        Some(json!({
            "email": "assigned@example.com",
            "name": "Assigned",
            "password": "long-enough-password",
            "role_ids": [assigned_id]
        })),
    )
    .await;
    assert_eq!(person.status(), StatusCode::CREATED);

    let assigned_delete = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/roles/{assigned_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(assigned_delete.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(assigned_delete).await["error"]["code"],
        "role_assigned"
    );

    let owner_delete = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/roles/{owner_role_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(owner_delete.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(owner_delete).await["error"]["code"],
        "owner_role_protected"
    );

    let owner_grants = send(
        &app,
        Method::PUT,
        &format!("/api/v1/roles/{owner_role_id}/grants"),
        Some(&owner_token),
        Some(json!({"grants": []})),
    )
    .await;
    assert_eq!(owner_grants.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(owner_grants).await["error"]["code"],
        "owner_role_protected"
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn api_key_lifecycle_lists_metadata_and_limits_assistant_revoke() {
    let (app, _database, _site_id) = support::build_app_with_database().await;
    let owner_token = bootstrap(&app).await;

    let first = send(
        &app,
        Method::POST,
        "/api/v1/auth/api-keys",
        Some(&owner_token),
        Some(json!({
            "name": "automation",
            "grants": [{"capability": "people", "action": "delete"}]
        })),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first = response_json(first).await;
    assert!(
        first["token"]
            .as_str()
            .is_some_and(|token| token.starts_with("mavi_key_"))
    );
    assert_eq!(first["prefix"].as_str().expect("key prefix").len(), 16);
    assert!(first.get("secret_hash").is_none());
    let first_id = first["id"].as_str().expect("first key id").to_owned();
    let first_token = first["token"].as_str().expect("first key token").to_owned();

    let second = send(
        &app,
        Method::POST,
        "/api/v1/auth/api-keys",
        Some(&owner_token),
        Some(json!({
            "name": "secondary",
            "grants": [{"capability": "content", "action": "view"}]
        })),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second_id = response_json(second).await["id"]
        .as_str()
        .expect("second key id")
        .to_owned();

    let listed = send(
        &app,
        Method::GET,
        "/api/v1/auth/api-keys?limit=10&revoked=false",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = response_json(listed).await;
    assert_eq!(listed["items"].as_array().expect("key items").len(), 2);
    assert!(
        listed["items"]
            .as_array()
            .expect("key items")
            .iter()
            .all(|key| { key.get("token").is_none() && key.get("secret_hash").is_none() })
    );

    let cross_revoke = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/auth/api-keys/{second_id}"),
        Some(&first_token),
        None,
    )
    .await;
    assert_eq!(cross_revoke.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response_json(cross_revoke).await["error"]["code"],
        "forbidden"
    );

    let self_revoke = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/auth/api-keys/{first_id}"),
        Some(&first_token),
        None,
    )
    .await;
    assert_eq!(self_revoke.status(), StatusCode::NO_CONTENT);
    let revoked_auth = send(
        &app,
        Method::GET,
        "/api/v1/people",
        Some(&first_token),
        None,
    )
    .await;
    assert_eq!(revoked_auth.status(), StatusCode::UNAUTHORIZED);

    let second_revoke = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/auth/api-keys/{second_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(second_revoke.status(), StatusCode::NO_CONTENT);

    let revoked = send(
        &app,
        Method::GET,
        "/api/v1/auth/api-keys?revoked=true",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(revoked.status(), StatusCode::OK);
    let revoked = response_json(revoked).await;
    assert_eq!(revoked["items"].as_array().expect("revoked keys").len(), 2);
    assert!(
        revoked["items"]
            .as_array()
            .expect("revoked keys")
            .iter()
            .all(|key| { key["revoked_at"].is_string() && key.get("token").is_none() })
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn edge_auth_throttle_is_site_scoped_and_audited_without_raw_source_data() {
    let (app, database, site_id) = support::build_app_with_edge_policy(EdgeThrottlePolicy {
        ip_limit: 2,
        ip_window: Duration::from_mins(1),
        device_limit: 100,
        device_window: Duration::from_mins(10),
        max_buckets: 32,
    })
    .await;
    let _owner_token = bootstrap(&app).await;
    let peer = "198.51.100.10:4242".parse().expect("peer");

    for _ in 0..2 {
        let response = send_with_peer(
            &app,
            Method::POST,
            "/api/v1/auth/sessions",
            peer,
            Some(json!({
                "email": "missing@example.com",
                "password": "long-enough-password"
            })),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let limited = send_with_peer(
        &app,
        Method::POST,
        "/api/v1/auth/sessions",
        peer,
        Some(json!({
            "email": "missing@example.com",
            "password": "long-enough-password"
        })),
    )
    .await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(limited.headers()["retry-after"], "60");
    assert_eq!(
        response_json(limited).await["error"]["code"],
        "rate_limited"
    );

    let public_context = SiteContext::public(site_id);
    let mut audit_tx = database.begin(&public_context).await.expect("audit scope");
    let (count, scope): (i64, Option<String>) = sqlx::query_as(
        "select count(*), max(payload->>'scope')
           from audit_events
          where site_id = $1 and action = 'auth.security.edge_rate_limited'",
    )
    .bind(site_id.into_uuid())
    .fetch_one(audit_tx.conn())
    .await
    .expect("edge audit");
    let payload: serde_json::Value = sqlx::query_scalar(
        "select payload
           from audit_events
          where site_id = $1 and action = 'auth.security.edge_rate_limited'
          order by created_at desc limit 1",
    )
    .bind(site_id.into_uuid())
    .fetch_one(audit_tx.conn())
    .await
    .expect("edge audit payload");
    assert_eq!(count, 1);
    assert_eq!(scope.as_deref(), Some("ip"));
    assert_eq!(payload["action"], "auth.session.create");
    assert!(payload["fingerprint"].as_str().is_some());
    assert!(!payload.to_string().contains("198.51.100.10"));
    audit_tx.commit().await.expect("audit commit");
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

    let role_delete = send(
        app,
        Method::DELETE,
        "/api/v1/roles/00000000-0000-7000-8000-000000000001",
        Some(reader_token),
        None,
    )
    .await;
    assert_eq!(role_delete.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response_json(role_delete).await["error"]["code"],
        "forbidden"
    );
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
          where site_id = $1 and action = 'auth.security.subject_rate_limited'",
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
