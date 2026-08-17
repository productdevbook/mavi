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
async fn taxonomy_routes_enforce_tree_assignment_cursor_and_permission_contracts() {
    let app = support::build_app().await;
    let owner_token = bootstrap(&app, "HTTP taxonomy test").await;
    let reader_token = create_reader(&app, &owner_token).await;

    let parent = create_term(
        &app,
        &owner_token,
        json!({
            "kind": "category",
            "language": "en",
            "slug": "news",
            "name": "News"
        }),
    )
    .await;
    let parent_id = parent["id"].as_str().expect("parent id").to_owned();
    let child = create_term(
        &app,
        &owner_token,
        json!({
            "kind": "category",
            "language": "en",
            "slug": "local",
            "name": "Local",
            "parent_id": parent_id
        }),
    )
    .await;
    let child_id = child["id"].as_str().expect("child id").to_owned();
    let tag = create_term(
        &app,
        &owner_token,
        json!({
            "kind": "tag",
            "language": "en",
            "slug": "featured",
            "name": "Featured"
        }),
    )
    .await;
    let tag_id = tag["id"].as_str().expect("tag id").to_owned();
    let _second_root = create_term(
        &app,
        &owner_token,
        json!({
            "kind": "category",
            "language": "en",
            "slug": "guides",
            "name": "Guides"
        }),
    )
    .await;

    let invalid_tag_parent = send(
        &app,
        Method::POST,
        "/api/v1/terms",
        Some(&owner_token),
        Some(json!({
            "kind": "tag",
            "language": "en",
            "slug": "invalid-parent",
            "name": "Invalid parent",
            "parent_id": parent_id
        })),
    )
    .await;
    assert_eq!(invalid_tag_parent.status(), StatusCode::BAD_REQUEST);

    let roots = send(
        &app,
        Method::GET,
        "/api/v1/terms?roots=true&kind=category&limit=1",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(roots.status(), StatusCode::OK);
    let roots = response_json(roots).await;
    assert_eq!(roots["items"].as_array().expect("items").len(), 1);
    let cursor = roots["next_cursor"]
        .as_str()
        .expect("root cursor")
        .to_owned();
    let second_roots = send(
        &app,
        Method::GET,
        &format!("/api/v1/terms?roots=true&kind=category&limit=1&after={cursor}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(second_roots.status(), StatusCode::OK);
    assert_eq!(
        response_json(second_roots).await["items"]
            .as_array()
            .expect("items")
            .len(),
        1
    );

    let cycle = send(
        &app,
        Method::PATCH,
        &format!("/api/v1/terms/{parent_id}"),
        Some(&owner_token),
        Some(json!({"parent_id": child_id})),
    )
    .await;
    assert_eq!(cycle.status(), StatusCode::BAD_REQUEST);

    let content = send(
        &app,
        Method::POST,
        "/api/v1/content",
        Some(&owner_token),
        Some(json!({
            "kind": "post",
            "language": "en",
            "slug": "taxonomy-content",
            "title": "Taxonomy content"
        })),
    )
    .await;
    assert_eq!(content.status(), StatusCode::CREATED);
    let content_id = response_json(content).await["id"]
        .as_str()
        .expect("content id")
        .to_owned();

    let assigned = send(
        &app,
        Method::PUT,
        &format!("/api/v1/content/{content_id}/terms"),
        Some(&owner_token),
        Some(json!({"term_ids": [parent_id, tag_id, tag_id]})),
    )
    .await;
    assert_eq!(assigned.status(), StatusCode::OK);
    assert_eq!(
        response_json(assigned)
            .await
            .as_array()
            .expect("terms")
            .len(),
        2
    );

    let content_terms = send(
        &app,
        Method::GET,
        &format!("/api/v1/content/{content_id}/terms"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(content_terms.status(), StatusCode::OK);
    assert_eq!(
        response_json(content_terms)
            .await
            .as_array()
            .expect("terms")
            .len(),
        2
    );

    let term_content = send(
        &app,
        Method::GET,
        &format!("/api/v1/terms/{parent_id}/content?limit=1"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(term_content.status(), StatusCode::OK);
    let term_content = response_json(term_content).await;
    assert_eq!(term_content["items"].as_array().expect("items").len(), 1);
    assert_eq!(term_content["items"][0]["content_id"], content_id);

    let invalid_cursor = send(
        &app,
        Method::GET,
        &format!("/api/v1/terms/{parent_id}/content?after=not-a-cursor"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(invalid_cursor.status(), StatusCode::BAD_REQUEST);

    let reader_terms = send(
        &app,
        Method::GET,
        "/api/v1/terms?limit=1",
        Some(&reader_token),
        None,
    )
    .await;
    assert_eq!(reader_terms.status(), StatusCode::OK);
    let reader_write = send(
        &app,
        Method::POST,
        "/api/v1/terms",
        Some(&reader_token),
        Some(json!({
            "kind": "tag",
            "language": "en",
            "slug": "reader-write",
            "name": "Reader write"
        })),
    )
    .await;
    assert_eq!(reader_write.status(), StatusCode::FORBIDDEN);

    let deleted = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/terms/{parent_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let remaining = send(
        &app,
        Method::GET,
        &format!("/api/v1/content/{content_id}/terms"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(remaining.status(), StatusCode::OK);
    let remaining = response_json(remaining).await;
    assert_eq!(remaining.as_array().expect("terms").len(), 1);
    assert_eq!(remaining[0]["id"], tag_id);
}

async fn create_term(app: &Router, token: &str, body: serde_json::Value) -> serde_json::Value {
    let response = send(app, Method::POST, "/api/v1/terms", Some(token), Some(body)).await;
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
            "name": "taxonomy-reader",
            "grants": [{"capability": "taxonomy", "action": "view"}]
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
            "email": "taxonomy-reader@example.com",
            "name": "Taxonomy reader",
            "password": "long-enough-password",
            "role_ids": [role_id]
        })),
    )
    .await;
    assert_eq!(person.status(), StatusCode::CREATED);
    login(app, "taxonomy-reader@example.com").await
}
