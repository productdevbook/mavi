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
async fn content_type_routes_validate_fields_and_preserve_content() {
    let app = support::build_app().await;
    let owner_token = bootstrap(&app, "HTTP content types test").await;
    let reader_token = create_reader(&app, &owner_token).await;

    let unauthenticated = send(&app, Method::GET, "/api/v1/content-types", None, None).await;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let first_page = send(
        &app,
        Method::GET,
        "/api/v1/content-types?limit=1",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(first_page.status(), StatusCode::OK);
    let first_page = response_json(first_page).await;
    assert_eq!(first_page["items"].as_array().expect("items").len(), 1);
    let cursor = first_page["next_cursor"]
        .as_str()
        .expect("content type cursor");
    let second_page = send(
        &app,
        Method::GET,
        &format!("/api/v1/content-types?limit=1&after={cursor}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(second_page.status(), StatusCode::OK);
    assert_eq!(
        response_json(second_page).await["items"]
            .as_array()
            .expect("items")
            .len(),
        1
    );

    let declared = send(
        &app,
        Method::PUT,
        "/api/v1/content-types/recipe",
        Some(&owner_token),
        Some(json!({
            "name": "Recipe",
            "fields": [
                {"key": "summary", "label": "Summary", "required": true, "kind": "text", "options": []},
                {"key": "status", "label": "Status", "required": false, "kind": "choice", "options": ["draft", "ready"]}
            ]
        })),
    )
    .await;
    assert_eq!(declared.status(), StatusCode::OK);
    assert_eq!(response_json(declared).await["kind"], "recipe");

    let missing_required = send(
        &app,
        Method::POST,
        "/api/v1/content",
        Some(&owner_token),
        Some(json!({
            "kind": "recipe",
            "language": "en",
            "slug": "missing-summary",
            "title": "Missing summary",
            "fields": {"status": "draft"}
        })),
    )
    .await;
    assert_eq!(missing_required.status(), StatusCode::BAD_REQUEST);
    let missing_required = response_json(missing_required).await;
    assert_eq!(missing_required["error"]["code"], "content_field_required");
    assert_eq!(missing_required["error"]["field"], "summary");

    let created = send(
        &app,
        Method::POST,
        "/api/v1/content",
        Some(&owner_token),
        Some(json!({
            "kind": "recipe",
            "language": "en",
            "slug": "valid-recipe",
            "title": "Valid recipe",
            "body": "Mix it.",
            "fields": {"summary": "A valid recipe", "status": "ready"}
        })),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let content_id = response_json(created).await["id"]
        .as_str()
        .expect("content id")
        .to_owned();

    let reader_list = send(
        &app,
        Method::GET,
        "/api/v1/content-types?limit=1",
        Some(&reader_token),
        None,
    )
    .await;
    assert_eq!(reader_list.status(), StatusCode::OK);

    let reader_write = send(
        &app,
        Method::PUT,
        "/api/v1/content-types/other",
        Some(&reader_token),
        Some(json!({"name": "Other"})),
    )
    .await;
    assert_eq!(reader_write.status(), StatusCode::FORBIDDEN);

    let deleted = send(
        &app,
        Method::DELETE,
        "/api/v1/content-types/recipe",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let retained = send(
        &app,
        Method::GET,
        &format!("/api/v1/content/{content_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(retained.status(), StatusCode::OK);
}

async fn create_reader(app: &Router, owner_token: &str) -> String {
    let role = send(
        app,
        Method::POST,
        "/api/v1/roles",
        Some(owner_token),
        Some(json!({
            "name": "content-type-reader",
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
            "email": "content-type-reader@example.com",
            "name": "Content type reader",
            "password": "long-enough-password",
            "role_ids": [role_id]
        })),
    )
    .await;
    assert_eq!(person.status(), StatusCode::CREATED);

    login(app, "content-type-reader@example.com").await
}
