use axum::{
    Router,
    http::{Method, StatusCode},
};
use serde_json::json;

mod support;
use support::{bootstrap, response_json, send};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn form_routes_validate_public_submissions_cursor_inbox_and_permissions() {
    let app = support::build_app().await;
    let owner_token = bootstrap(&app, "HTTP forms test").await;

    let form = send(
        &app,
        Method::POST,
        "/api/v1/forms",
        Some(&owner_token),
        Some(json!({
            "slug": "contact",
            "name": "Contact us",
            "fields": [
                {"key": "name", "label": "Name", "required": true, "kind": "text"},
                {"key": "email", "label": "Email", "required": true, "kind": "email"},
                {"key": "topic", "label": "Topic", "required": true, "kind": "choice", "options": ["sales", "support"]}
            ],
            "kept_days": 30
        })),
    )
    .await;
    assert_eq!(form.status(), StatusCode::CREATED);
    let form_id = response_json(form).await["id"]
        .as_str()
        .expect("form id")
        .to_owned();

    let second_form = send(
        &app,
        Method::POST,
        "/api/v1/forms",
        Some(&owner_token),
        Some(json!({"slug": "feedback", "name": "Feedback"})),
    )
    .await;
    assert_eq!(second_form.status(), StatusCode::CREATED);

    let forms = send(
        &app,
        Method::GET,
        "/api/v1/forms?limit=1",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(forms.status(), StatusCode::OK);
    let forms = response_json(forms).await;
    assert_eq!(forms["items"].as_array().expect("form items").len(), 1);
    let cursor = forms["next_cursor"].as_str().expect("form cursor");
    assert!(!cursor.contains("page") && !cursor.contains("offset"));
    let next = send(
        &app,
        Method::GET,
        &format!("/api/v1/forms?limit=1&after={cursor}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(next.status(), StatusCode::OK);

    let public = send(&app, Method::GET, "/public/v1/forms/contact", None, None).await;
    assert_eq!(public.status(), StatusCode::OK);
    assert_eq!(response_json(public).await["slug"], "contact");

    let invalid = send(
        &app,
        Method::POST,
        "/public/v1/forms/contact/submissions",
        None,
        Some(json!({"answers": {"name": "Visitor", "email": "visitor@example.test", "topic": "billing"}})),
    )
    .await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let submitted = send(
        &app,
        Method::POST,
        "/public/v1/forms/contact/submissions",
        None,
        Some(json!({"answers": {"name": "Visitor", "email": "visitor@example.test", "topic": "support"}})),
    )
    .await;
    assert_eq!(submitted.status(), StatusCode::CREATED);
    let submission_id = response_json(submitted).await["id"]
        .as_str()
        .expect("submission id")
        .to_owned();

    let unread = send(
        &app,
        Method::GET,
        &format!("/api/v1/forms/{form_id}/submissions?limit=1&unread=true"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(unread.status(), StatusCode::OK);
    let unread = response_json(unread).await;
    assert_eq!(unread["items"].as_array().expect("unread items").len(), 1);
    assert!(unread["next_cursor"].is_null());

    let export = send(
        &app,
        Method::GET,
        &format!("/api/v1/forms/{form_id}/submissions/export?limit=1"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(export.status(), StatusCode::OK);
    let export = response_json(export).await;
    assert_eq!(export["format"], "mavi.forms.submissions");
    assert_eq!(export["version"], 1);
    assert_eq!(export["form"]["id"], form_id);
    assert_eq!(export["items"].as_array().expect("export items").len(), 1);
    assert!(export["next_cursor"].is_null());

    let marked = send(
        &app,
        Method::POST,
        &format!("/api/v1/forms/{form_id}/submissions/mark-read"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(marked.status(), StatusCode::OK);
    assert_eq!(response_json(marked).await["seen"], 1);

    let invalid_cursor = send(
        &app,
        Method::GET,
        &format!("/api/v1/forms/{form_id}/submissions?after=not-a-cursor"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(invalid_cursor.status(), StatusCode::BAD_REQUEST);

    let deleted = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/form-submissions/{submission_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let reader_token = create_reader(&app, &owner_token).await;
    let reader_forms = send(
        &app,
        Method::GET,
        "/api/v1/forms",
        Some(&reader_token),
        None,
    )
    .await;
    assert_eq!(reader_forms.status(), StatusCode::OK);
    let reader_write = send(
        &app,
        Method::POST,
        "/api/v1/forms",
        Some(&reader_token),
        Some(json!({"slug": "reader", "name": "Reader cannot write"})),
    )
    .await;
    assert_eq!(reader_write.status(), StatusCode::FORBIDDEN);
}

async fn create_reader(app: &Router, owner_token: &str) -> String {
    let role = send(
        app,
        Method::POST,
        "/api/v1/roles",
        Some(owner_token),
        Some(json!({
            "name": "forms-reader",
            "grants": [{"capability": "forms", "action": "view"}]
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
            "email": "forms-reader@example.com",
            "name": "Forms Reader",
            "password": "long-enough-password",
            "role_ids": [role_id]
        })),
    )
    .await;
    assert_eq!(person.status(), StatusCode::CREATED);
    support::verify_email(
        app,
        &response_json(person).await,
        "forms-reader@example.com",
    )
    .await;
    support::login(app, "forms-reader@example.com").await
}
