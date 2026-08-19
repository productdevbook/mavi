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

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn public_taxonomy_archives_use_language_fallback_and_cursor_pagination() {
    let app = support::build_app().await;
    let owner_token = bootstrap(&app, "HTTP public taxonomy archive test").await;

    let language = send(
        &app,
        Method::POST,
        "/api/v1/languages",
        Some(&owner_token),
        Some(json!({
            "tag": "de",
            "name": "Deutsch",
            "is_default": false
        })),
    )
    .await;
    assert_eq!(language.status(), StatusCode::CREATED);

    let english_term = create_term(
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
    let german_term = create_term(
        &app,
        &owner_token,
        json!({
            "kind": "category",
            "language": "de",
            "slug": "news",
            "name": "Nachrichten"
        }),
    )
    .await;

    let german_term_id = german_term["id"].as_str().expect("German term id");
    let english_term_id = english_term["id"].as_str().expect("English term id");
    create_published_content(
        &app,
        &owner_token,
        "de",
        "german-news-one",
        "Deutsche Nachricht eins",
        german_term_id,
    )
    .await;
    create_published_content(
        &app,
        &owner_token,
        "de",
        "german-news-two",
        "Deutsche Nachricht zwei",
        german_term_id,
    )
    .await;
    create_published_content(
        &app,
        &owner_token,
        "en",
        "english-news",
        "English news",
        english_term_id,
    )
    .await;

    let draft = send(
        &app,
        Method::POST,
        "/api/v1/content",
        Some(&owner_token),
        Some(json!({
            "kind": "post",
            "language": "de",
            "slug": "german-draft",
            "title": "Draft must stay private"
        })),
    )
    .await;
    assert_eq!(draft.status(), StatusCode::CREATED);
    let draft_id = response_json(draft).await["id"]
        .as_str()
        .expect("draft id")
        .to_owned();
    let assigned = send(
        &app,
        Method::PUT,
        &format!("/api/v1/content/{draft_id}/terms"),
        Some(&owner_token),
        Some(json!({"term_ids": [german_term_id]})),
    )
    .await;
    assert_eq!(assigned.status(), StatusCode::OK);

    let regional = send(
        &app,
        Method::GET,
        "/public/v1/terms/category/news?language=de-DE&limit=1",
        None,
        None,
    )
    .await;
    assert_eq!(regional.status(), StatusCode::OK);
    let regional = response_json(regional).await;
    assert_eq!(regional["items"].as_array().expect("items").len(), 1);
    assert_eq!(regional["items"][0]["language"], "de");
    let cursor = regional["next_cursor"]
        .as_str()
        .expect("archive cursor")
        .to_owned();

    let second_page = send(
        &app,
        Method::GET,
        &format!("/public/v1/terms/category/news?language=de-DE&limit=1&after={cursor}"),
        None,
        None,
    )
    .await;
    assert_eq!(second_page.status(), StatusCode::OK);
    let second_page = response_json(second_page).await;
    assert_eq!(second_page["items"].as_array().expect("items").len(), 1);
    assert_eq!(second_page["items"][0]["language"], "de");
    assert!(second_page["next_cursor"].is_null());

    let default = send(
        &app,
        Method::GET,
        "/public/v1/terms/category/news?language=fr-FR",
        None,
        None,
    )
    .await;
    assert_eq!(default.status(), StatusCode::OK);
    let default = response_json(default).await;
    assert_eq!(default["items"].as_array().expect("items").len(), 1);
    assert_eq!(default["items"][0]["language"], "en");

    let missing = send(&app, Method::GET, "/public/v1/terms/tag/news", None, None).await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

async fn create_term(app: &Router, token: &str, body: serde_json::Value) -> serde_json::Value {
    let response = send(app, Method::POST, "/api/v1/terms", Some(token), Some(body)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await
}

async fn create_published_content(
    app: &Router,
    token: &str,
    language: &str,
    slug: &str,
    title: &str,
    term_id: &str,
) {
    let created = send(
        app,
        Method::POST,
        "/api/v1/content",
        Some(token),
        Some(json!({
            "kind": "post",
            "language": language,
            "slug": slug,
            "title": title
        })),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let id = response_json(created).await["id"]
        .as_str()
        .expect("content id")
        .to_owned();

    let published = send(
        app,
        Method::POST,
        &format!("/api/v1/content/{id}/publish"),
        Some(token),
        None,
    )
    .await;
    assert_eq!(published.status(), StatusCode::OK);

    let assigned = send(
        app,
        Method::PUT,
        &format!("/api/v1/content/{id}/terms"),
        Some(token),
        Some(json!({"term_ids": [term_id]})),
    )
    .await;
    assert_eq!(assigned.status(), StatusCode::OK);
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
    support::verify_email(
        app,
        &response_json(person).await,
        "taxonomy-reader@example.com",
    )
    .await;
    login(app, "taxonomy-reader@example.com").await
}
