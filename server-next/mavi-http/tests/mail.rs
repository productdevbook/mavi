use axum::{
    Router,
    http::{Method, StatusCode},
};
use serde_json::json;

mod support;
use support::{bootstrap, login, response_json, send};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn mail_routes_validate_templates_lists_unsubscribe_and_provider_neutral_outbox() {
    let app = support::build_app().await;
    let owner_token = bootstrap(&app, "HTTP mail test").await;

    let template = send(
        &app,
        Method::POST,
        "/api/v1/mail/templates",
        Some(&owner_token),
        Some(json!({
            "key": "welcome",
            "language": "en",
            "subject": "Hello {{name}}",
            "body": "Welcome {{name}}. Count: {{count}}"
        })),
    )
    .await;
    assert_eq!(template.status(), StatusCode::CREATED);
    let template = response_json(template).await;
    let template_id = template["id"].as_str().expect("template id").to_owned();
    assert_eq!(template["variables"], json!(["count", "name"]));

    let missing = send(
        &app,
        Method::POST,
        &format!("/api/v1/mail/templates/{template_id}/preview"),
        Some(&owner_token),
        Some(json!({"variables": {"name": "Ada"}})),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);

    let preview = send(
        &app,
        Method::POST,
        &format!("/api/v1/mail/templates/{template_id}/preview"),
        Some(&owner_token),
        Some(json!({"variables": {"name": "Ada", "count": 3}})),
    )
    .await;
    assert_eq!(preview.status(), StatusCode::OK);
    assert_eq!(response_json(preview).await["subject"], "Hello Ada");

    let list = send(
        &app,
        Method::POST,
        "/api/v1/mail/lists",
        Some(&owner_token),
        Some(json!({"slug": "updates", "name": "Product updates"})),
    )
    .await;
    assert_eq!(list.status(), StatusCode::CREATED);
    let list_id = response_json(list).await["id"]
        .as_str()
        .expect("list id")
        .to_owned();

    let first_reader = add_reader(&app, &owner_token, &list_id, "ada@example.test", "Ada").await;
    let first_token = first_reader["unsubscribe_token"]
        .as_str()
        .expect("unsubscribe token")
        .to_owned();
    let _second_reader =
        add_reader(&app, &owner_token, &list_id, "grace@example.test", "Grace").await;

    let readers = send(
        &app,
        Method::GET,
        &format!("/api/v1/mail/lists/{list_id}/readers?limit=1"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(readers.status(), StatusCode::OK);
    let readers = response_json(readers).await;
    assert_eq!(readers["items"].as_array().expect("reader items").len(), 1);
    let cursor = readers["next_cursor"].as_str().expect("reader cursor");
    assert!(!cursor.contains("page") && !cursor.contains("offset"));
    let next_readers = send(
        &app,
        Method::GET,
        &format!("/api/v1/mail/lists/{list_id}/readers?limit=1&after={cursor}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(next_readers.status(), StatusCode::OK);

    let unsubscribed = send(
        &app,
        Method::POST,
        &format!("/public/v1/mail/unsubscribe/{first_token}"),
        None,
        None,
    )
    .await;
    assert_eq!(unsubscribed.status(), StatusCode::OK);
    assert_eq!(response_json(unsubscribed).await["unsubscribed"], true);
    let unknown_unsubscribe = send(
        &app,
        Method::POST,
        "/public/v1/mail/unsubscribe/not-a-real-token",
        None,
        None,
    )
    .await;
    assert_eq!(unknown_unsubscribe.status(), StatusCode::OK);

    let settings = send(
        &app,
        Method::PATCH,
        "/api/v1/settings",
        Some(&owner_token),
        Some(json!({"canonical_url": "https://mail.example.test"})),
    )
    .await;
    assert_eq!(settings.status(), StatusCode::OK);

    let delivery = send(
        &app,
        Method::POST,
        "/api/v1/mail/deliveries",
        Some(&owner_token),
        Some(json!({
            "recipient": "grace@example.test",
            "template_id": template_id,
            "variables": {"name": "Grace", "count": 1},
            "idempotency_key": "welcome-grace-1"
        })),
    )
    .await;
    assert_eq!(delivery.status(), StatusCode::ACCEPTED);
    let delivery = response_json(delivery).await;
    let delivery_id = delivery["id"].as_str().expect("delivery id").to_owned();
    assert_eq!(delivery["status"], "queued");
    assert_eq!(delivery["recipient"], "grace@example.test");

    let duplicate = send(
        &app,
        Method::POST,
        "/api/v1/mail/deliveries",
        Some(&owner_token),
        Some(json!({
            "recipient": "grace@example.test",
            "template_id": template_id,
            "variables": {},
            "idempotency_key": "welcome-grace-1"
        })),
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::ACCEPTED);
    assert_eq!(response_json(duplicate).await["id"], delivery_id);

    let campaign = send(
        &app,
        Method::POST,
        &format!("/api/v1/mail/lists/{list_id}/deliveries"),
        Some(&owner_token),
        Some(json!({
            "template_id": template_id,
            "variables": {"name": "Reader", "count": 4},
            "idempotency_key": "campaign-1"
        })),
    )
    .await;
    assert_eq!(campaign.status(), StatusCode::ACCEPTED);
    assert_eq!(response_json(campaign).await["enqueued"], 1);

    let deliveries = send(
        &app,
        Method::GET,
        "/api/v1/mail/deliveries?limit=1",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(deliveries.status(), StatusCode::OK);
    let deliveries = response_json(deliveries).await;
    assert_eq!(
        deliveries["items"]
            .as_array()
            .expect("delivery items")
            .len(),
        1
    );
    assert!(
        !deliveries["next_cursor"]
            .as_str()
            .unwrap_or_default()
            .contains("offset")
    );

    let reader_token = create_reader(&app, &owner_token).await;
    let reader_view = send(
        &app,
        Method::GET,
        "/api/v1/mail/templates",
        Some(&reader_token),
        None,
    )
    .await;
    assert_eq!(reader_view.status(), StatusCode::OK);
    let reader_write = send(
        &app,
        Method::POST,
        "/api/v1/mail/lists",
        Some(&reader_token),
        Some(json!({"slug": "reader-cannot-write", "name": "No"})),
    )
    .await;
    assert_eq!(reader_write.status(), StatusCode::FORBIDDEN);
}

async fn add_reader(
    app: &Router,
    token: &str,
    list_id: &str,
    email: &str,
    name: &str,
) -> serde_json::Value {
    let response = send(
        app,
        Method::POST,
        &format!("/api/v1/mail/lists/{list_id}/readers"),
        Some(token),
        Some(json!({"email": email, "name": name})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await
}

async fn create_reader(app: &Router, owner_token: &str) -> String {
    let role = send(
        app,
        Method::POST,
        "/api/v1/roles",
        Some(owner_token),
        Some(json!({
            "name": "mail-reader",
            "grants": [{"capability": "mail", "action": "view"}]
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
            "email": "mail-reader@example.com",
            "name": "Mail Reader",
            "password": "long-enough-password",
            "role_ids": [role_id]
        })),
    )
    .await;
    assert_eq!(person.status(), StatusCode::CREATED);
    support::verify_email(app, &response_json(person).await, "mail-reader@example.com").await;
    login(app, "mail-reader@example.com").await
}
