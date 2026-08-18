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
async fn content_lifecycle_exposes_revisions_and_keeps_old_public_paths() {
    let app = support::build_app().await;
    let owner_token = bootstrap(&app, "HTTP content lifecycle test").await;
    let reader_token = create_reader(&app, &owner_token).await;

    let created = send(
        &app,
        Method::POST,
        "/api/v1/content",
        Some(&owner_token),
        Some(json!({
            "kind": "post",
            "language": "en",
            "slug": "old-public-path",
            "title": "First title",
            "body": "First body"
        })),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let content_id = response_json(created).await["id"]
        .as_str()
        .expect("content id")
        .to_owned();

    let updated = send(
        &app,
        Method::PATCH,
        &format!("/api/v1/content/{content_id}"),
        Some(&owner_token),
        Some(json!({
            "slug": "new-public-path",
            "title": "Updated title"
        })),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(response_json(updated).await["revision"], 2);

    let published = send(
        &app,
        Method::POST,
        &format!("/api/v1/content/{content_id}/publish"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(published.status(), StatusCode::OK);
    assert_eq!(response_json(published).await["revision"], 3);

    let first_page = send(
        &app,
        Method::GET,
        &format!("/api/v1/content/{content_id}/revisions?limit=1"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(first_page.status(), StatusCode::OK);
    let first_page = response_json(first_page).await;
    assert_eq!(first_page["items"].as_array().expect("items").len(), 1);
    assert_eq!(first_page["items"][0]["revision"], 3);
    let cursor = first_page["next_cursor"]
        .as_str()
        .expect("revision cursor")
        .to_owned();

    let second_page = send(
        &app,
        Method::GET,
        &format!("/api/v1/content/{content_id}/revisions?limit=1&after={cursor}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(second_page.status(), StatusCode::OK);
    let second_page = response_json(second_page).await;
    assert_eq!(second_page["items"][0]["revision"], 2);

    let invalid_cursor = send(
        &app,
        Method::GET,
        &format!("/api/v1/content/{content_id}/revisions?after=not-a-cursor"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(invalid_cursor.status(), StatusCode::BAD_REQUEST);

    let revision = send(
        &app,
        Method::GET,
        &format!("/api/v1/content/{content_id}/revisions/1"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(revision.status(), StatusCode::OK);
    let revision = response_json(revision).await;
    assert_eq!(revision["revision"], 1);
    assert_eq!(revision["slug"], "old-public-path");
    assert_eq!(revision["title"], "First title");

    let invalid_revision = send(
        &app,
        Method::GET,
        &format!("/api/v1/content/{content_id}/revisions/0"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(invalid_revision.status(), StatusCode::NOT_FOUND);

    let old_public_path = send(
        &app,
        Method::GET,
        "/public/v1/content/old-public-path",
        None,
        None,
    )
    .await;
    assert_eq!(old_public_path.status(), StatusCode::OK);
    let old_public_path = response_json(old_public_path).await;
    assert_eq!(old_public_path["id"], content_id);
    assert_eq!(old_public_path["slug"], "new-public-path");

    let reader_revisions = send(
        &app,
        Method::GET,
        &format!("/api/v1/content/{content_id}/revisions?limit=1"),
        Some(&reader_token),
        None,
    )
    .await;
    assert_eq!(reader_revisions.status(), StatusCode::OK);

    let reader_update = send(
        &app,
        Method::PATCH,
        &format!("/api/v1/content/{content_id}"),
        Some(&reader_token),
        Some(json!({"title": "Reader must not update"})),
    )
    .await;
    assert_eq!(reader_update.status(), StatusCode::FORBIDDEN);
}

async fn create_reader(app: &Router, owner_token: &str) -> String {
    let role = send(
        app,
        Method::POST,
        "/api/v1/roles",
        Some(owner_token),
        Some(json!({
            "name": "content-lifecycle-reader",
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
            "email": "content-lifecycle-reader@example.com",
            "name": "Content lifecycle reader",
            "password": "long-enough-password",
            "role_ids": [role_id]
        })),
    )
    .await;
    assert_eq!(person.status(), StatusCode::CREATED);
    support::verify_email(
        app,
        &response_json(person).await,
        "content-lifecycle-reader@example.com",
    )
    .await;

    login(app, "content-lifecycle-reader@example.com").await
}
