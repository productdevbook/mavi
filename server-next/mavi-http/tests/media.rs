use axum::{
    Router,
    http::{Method, StatusCode},
};
use serde_json::json;

mod support;
use support::{bootstrap, response_json, send, send_raw};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn media_routes_detect_bytes_page_with_cursor_and_enforce_grants() {
    let app = support::build_app().await;
    let owner_token = bootstrap(&app, "HTTP media test").await;

    let first = send_raw(
        &app,
        Method::POST,
        "/api/v1/files?name=holiday.png",
        Some(&owner_token),
        "application/octet-stream",
        PNG.to_vec(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first = response_json(first).await;
    assert_eq!(first["kind"], "image");
    assert_eq!(first["mime"], "image/png");
    assert_eq!(first["name"], "holiday.png");
    assert_eq!(first["bytes"], PNG.len());
    let first_id = first["id"].as_str().expect("file id").to_owned();

    let second = send_raw(
        &app,
        Method::POST,
        "/api/v1/files?name=second.png",
        Some(&owner_token),
        "application/octet-stream",
        PNG.to_vec(),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);

    let page = send(
        &app,
        Method::GET,
        "/api/v1/files?limit=1&kind=image",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(page.status(), StatusCode::OK);
    let page = response_json(page).await;
    assert_eq!(page["items"].as_array().expect("items").len(), 1);
    let cursor = page["next_cursor"]
        .as_str()
        .expect("opaque cursor")
        .to_owned();
    assert!(!cursor.contains("page") && !cursor.contains("offset"));

    let next = send(
        &app,
        Method::GET,
        &format!("/api/v1/files?limit=1&after={cursor}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(next.status(), StatusCode::OK);
    assert_eq!(
        response_json(next).await["items"]
            .as_array()
            .expect("items")
            .len(),
        1
    );

    let read = send(
        &app,
        Method::GET,
        &format!("/api/v1/files/{first_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(read.status(), StatusCode::OK);

    let invalid = send_raw(
        &app,
        Method::POST,
        "/api/v1/files?name=empty.bin",
        Some(&owner_token),
        "application/octet-stream",
        Vec::<u8>::new(),
    )
    .await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let reader_token = create_reader(&app, &owner_token).await;
    let reader_list = send(
        &app,
        Method::GET,
        "/api/v1/files",
        Some(&reader_token),
        None,
    )
    .await;
    assert_eq!(reader_list.status(), StatusCode::OK);
    let reader_upload = send_raw(
        &app,
        Method::POST,
        "/api/v1/files?name=reader.png",
        Some(&reader_token),
        "application/octet-stream",
        PNG.to_vec(),
    )
    .await;
    assert_eq!(reader_upload.status(), StatusCode::FORBIDDEN);

    let deleted = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/files/{first_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let missing = send(
        &app,
        Method::GET,
        &format!("/api/v1/files/{first_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

async fn create_reader(app: &Router, owner_token: &str) -> String {
    let role = send(
        app,
        Method::POST,
        "/api/v1/roles",
        Some(owner_token),
        Some(json!({
            "name": "media-reader",
            "grants": [{"capability": "media", "action": "view"}]
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
            "email": "media-reader@example.com",
            "name": "Media Reader",
            "password": "long-enough-password",
            "role_ids": [role_id]
        })),
    )
    .await;
    assert_eq!(person.status(), StatusCode::CREATED);
    support::login(app, "media-reader@example.com").await
}
