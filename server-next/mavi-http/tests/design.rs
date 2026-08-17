use axum::{
    Router,
    http::{Method, StatusCode},
};
use serde_json::json;

mod support;
use support::{bootstrap, login, response_bytes, response_json, send};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn design_routes_use_opaque_cursors_build_immutable_previews_and_rollback() {
    let app = support::build_app().await;
    let owner_token = bootstrap(&app, "HTTP design test").await;

    let started = send(
        &app,
        Method::POST,
        "/api/v1/design/changes",
        Some(&owner_token),
        Some(json!({"name": "First design"})),
    )
    .await;
    assert_eq!(started.status(), StatusCode::CREATED);
    let first_id = response_json(started).await["id"]
        .as_str()
        .expect("first change id")
        .to_owned();

    let index = send(
        &app,
        Method::PUT,
        &format!("/api/v1/design/changes/{first_id}/file"),
        Some(&owner_token),
        Some(json!({
            "path": "public/index.html",
            "contents": "<h1>first</h1>"
        })),
    )
    .await;
    assert_eq!(index.status(), StatusCode::OK);
    let source = send(
        &app,
        Method::PUT,
        &format!("/api/v1/design/changes/{first_id}/file"),
        Some(&owner_token),
        Some(json!({
            "path": "src/main.ts",
            "contents": "console.log('not public');"
        })),
    )
    .await;
    assert_eq!(source.status(), StatusCode::OK);

    let files = send(
        &app,
        Method::GET,
        &format!("/api/v1/design/changes/{first_id}/files?limit=1"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(files.status(), StatusCode::OK);
    let files = response_json(files).await;
    assert_eq!(files["items"].as_array().expect("file items").len(), 1);
    let file_cursor = files["next_cursor"].as_str().expect("file cursor");
    assert!(!file_cursor.contains("offset") && !file_cursor.contains("page"));

    let read = send(
        &app,
        Method::GET,
        &format!("/api/v1/design/changes/{first_id}/file?path=public%2Findex.html"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(read.status(), StatusCode::OK);
    assert_eq!(response_json(read).await["contents"], "<h1>first</h1>");

    let invalid = send(
        &app,
        Method::PUT,
        &format!("/api/v1/design/changes/{first_id}/file"),
        Some(&owner_token),
        Some(json!({"path": "public/../secret", "contents": "x"})),
    )
    .await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let build = send(
        &app,
        Method::POST,
        &format!("/api/v1/design/changes/{first_id}/builds"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(build.status(), StatusCode::CREATED);
    let build = response_json(build).await;
    assert_eq!(build["state"], "ready");
    let build_id = build["id"].as_str().expect("build id").to_owned();

    let preview = send(
        &app,
        Method::GET,
        &format!("/preview/v1/design/{build_id}/index.html"),
        None,
        None,
    )
    .await;
    assert_eq!(preview.status(), StatusCode::OK);
    assert_eq!(response_bytes(preview).await, b"<h1>first</h1>");

    let published = send(
        &app,
        Method::POST,
        &format!("/api/v1/design/changes/{first_id}/publish"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(published.status(), StatusCode::OK);
    assert_eq!(response_json(published).await["state"], "published");

    let public = send(&app, Method::GET, "/public/v1/site/index.html", None, None).await;
    assert_eq!(public.status(), StatusCode::OK);
    assert_eq!(response_bytes(public).await, b"<h1>first</h1>");

    let second = send(
        &app,
        Method::POST,
        "/api/v1/design/changes",
        Some(&owner_token),
        Some(json!({"name": "Second design"})),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second_id = response_json(second).await["id"]
        .as_str()
        .expect("second change id")
        .to_owned();
    let second_read = send(
        &app,
        Method::GET,
        &format!("/api/v1/design/changes/{second_id}/file?path=public%2Findex.html"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(second_read.status(), StatusCode::OK);
    assert_eq!(
        response_json(second_read).await["contents"],
        "<h1>first</h1>"
    );
    let second_write = send(
        &app,
        Method::PUT,
        &format!("/api/v1/design/changes/{second_id}/file"),
        Some(&owner_token),
        Some(json!({
            "path": "public/index.html",
            "contents": "<h1>second</h1>"
        })),
    )
    .await;
    assert_eq!(second_write.status(), StatusCode::OK);
    let second_build = send(
        &app,
        Method::POST,
        &format!("/api/v1/design/changes/{second_id}/builds"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(second_build.status(), StatusCode::CREATED);
    assert_eq!(response_json(second_build).await["state"], "ready");
    let second_published = send(
        &app,
        Method::POST,
        &format!("/api/v1/design/changes/{second_id}/publish"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(second_published.status(), StatusCode::OK);

    let rollback = send(
        &app,
        Method::POST,
        &format!("/api/v1/design/changes/{first_id}/rollback"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(rollback.status(), StatusCode::OK);
    let public_after_rollback =
        send(&app, Method::GET, "/public/v1/site/index.html", None, None).await;
    assert_eq!(public_after_rollback.status(), StatusCode::OK);
    assert_eq!(
        response_bytes(public_after_rollback).await,
        b"<h1>first</h1>"
    );

    let published_write = send(
        &app,
        Method::PUT,
        &format!("/api/v1/design/changes/{first_id}/file"),
        Some(&owner_token),
        Some(json!({"path": "public/index.html", "contents": "no"})),
    )
    .await;
    assert_eq!(published_write.status(), StatusCode::CONFLICT);

    let list = send(
        &app,
        Method::GET,
        "/api/v1/design/changes?limit=1",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let list = response_json(list).await;
    let cursor = list["next_cursor"].as_str().expect("change cursor");
    assert!(!cursor.contains("offset") && !cursor.contains("page"));
    let next = send(
        &app,
        Method::GET,
        &format!("/api/v1/design/changes?limit=1&after={cursor}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(next.status(), StatusCode::OK);

    let reader_token = create_reader(&app, &owner_token).await;
    let forbidden = send(
        &app,
        Method::GET,
        "/api/v1/design/changes",
        Some(&reader_token),
        None,
    )
    .await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
}

async fn create_reader(app: &Router, owner_token: &str) -> String {
    let role = send(
        app,
        Method::POST,
        "/api/v1/roles",
        Some(owner_token),
        Some(json!({
            "name": "design-reader",
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
            "email": "design-reader@example.com",
            "name": "Design Reader",
            "password": "long-enough-password",
            "role_ids": [role_id]
        })),
    )
    .await;
    assert_eq!(person.status(), StatusCode::CREATED);
    login(app, "design-reader@example.com").await
}
