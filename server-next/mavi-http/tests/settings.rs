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
async fn settings_and_languages_enforce_cursor_authz_and_defaults() {
    let app = support::build_app().await;
    let owner_token = bootstrap(&app, "HTTP settings test").await;
    let reader_token = create_reader(&app, &owner_token).await;

    let unauthenticated = send(&app, Method::GET, "/api/v1/settings", None, None).await;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let settings = send(
        &app,
        Method::GET,
        "/api/v1/settings",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(settings.status(), StatusCode::OK);
    assert_eq!(response_json(settings).await["timezone"], "UTC");

    let updated = send(
        &app,
        Method::PATCH,
        "/api/v1/settings",
        Some(&owner_token),
        Some(json!({"name": "Updated settings site", "timezone": "Europe/Berlin"})),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let updated = response_json(updated).await;
    assert_eq!(updated["name"], "Updated settings site");
    assert_eq!(updated["timezone"], "Europe/Berlin");

    create_language(&app, &owner_token, "tr", "Türkçe", true).await;
    create_language(&app, &owner_token, "de", "Deutsch", false).await;

    let first_page = send(
        &app,
        Method::GET,
        "/api/v1/languages?limit=2",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(first_page.status(), StatusCode::OK);
    let first_page = response_json(first_page).await;
    assert_eq!(first_page["items"].as_array().expect("items").len(), 2);
    let cursor = first_page["next_cursor"]
        .as_str()
        .expect("next cursor from keyset page");

    let second_page = send(
        &app,
        Method::GET,
        &format!("/api/v1/languages?limit=2&after={cursor}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(second_page.status(), StatusCode::OK);
    let second_page = response_json(second_page).await;
    assert_eq!(second_page["items"].as_array().expect("items").len(), 1);
    assert!(second_page["next_cursor"].is_null());

    let invalid_cursor = send(
        &app,
        Method::GET,
        "/api/v1/languages?after=not-a-valid-cursor",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(invalid_cursor.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(invalid_cursor).await["error"]["code"],
        "invalid_cursor"
    );

    let promoted = send(
        &app,
        Method::PATCH,
        "/api/v1/languages/tr",
        Some(&owner_token),
        Some(json!({"is_default": true})),
    )
    .await;
    assert_eq!(promoted.status(), StatusCode::OK);
    assert!(
        response_json(promoted).await["is_default"]
            .as_bool()
            .expect("default flag")
    );

    let delete_default = send(
        &app,
        Method::DELETE,
        "/api/v1/languages/tr",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(delete_default.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(delete_default).await["error"]["code"],
        "default_language_required"
    );

    let delete_old_default = send(
        &app,
        Method::DELETE,
        "/api/v1/languages/en",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(delete_old_default.status(), StatusCode::NO_CONTENT);

    let reader_settings = send(
        &app,
        Method::GET,
        "/api/v1/settings",
        Some(&reader_token),
        None,
    )
    .await;
    assert_eq!(reader_settings.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response_json(reader_settings).await["error"]["code"],
        "forbidden"
    );
}

async fn create_reader(app: &Router, owner_token: &str) -> String {
    let role = send(
        app,
        Method::POST,
        "/api/v1/roles",
        Some(owner_token),
        Some(json!({
            "name": "settings-reader",
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
            "email": "settings-reader@example.com",
            "name": "Settings reader",
            "password": "long-enough-password",
            "role_ids": [role_id]
        })),
    )
    .await;
    assert_eq!(person.status(), StatusCode::CREATED);
    support::verify_email(
        app,
        &response_json(person).await,
        "settings-reader@example.com",
    )
    .await;

    login(app, "settings-reader@example.com").await
}

async fn create_language(app: &Router, owner_token: &str, tag: &str, name: &str, is_default: bool) {
    let response = send(
        app,
        Method::POST,
        "/api/v1/languages",
        Some(owner_token),
        Some(json!({"tag": tag, "name": name, "is_default": is_default})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}
