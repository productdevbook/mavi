use axum::{Router, http::Method, http::StatusCode};
use serde_json::json;

mod support;
use support::{bootstrap, response_json, send, send_raw};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn audit_and_trash_routes_use_cursors_restore_and_cleanup_boundaries() {
    let app = support::build_app().await;
    let owner_token = bootstrap(&app, "HTTP audit and trash test").await;

    let unauthenticated_audit = send(&app, Method::GET, "/api/v1/audit", None, None).await;
    assert_eq!(unauthenticated_audit.status(), StatusCode::UNAUTHORIZED);
    let unauthenticated_export = send(&app, Method::GET, "/api/v1/audit/export", None, None).await;
    assert_eq!(unauthenticated_export.status(), StatusCode::UNAUTHORIZED);
    let unauthenticated_trash = send(&app, Method::GET, "/api/v1/trash", None, None).await;
    assert_eq!(unauthenticated_trash.status(), StatusCode::UNAUTHORIZED);

    let first_content = create_content(&app, &owner_token, "audit-trash-first").await;
    let second_content = create_content(&app, &owner_token, "audit-trash-second").await;

    let first_audit_page = send(
        &app,
        Method::GET,
        "/api/v1/audit?limit=1",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(first_audit_page.status(), StatusCode::OK);
    let first_audit_page = response_json(first_audit_page).await;
    assert_eq!(
        first_audit_page["items"]
            .as_array()
            .expect("audit items")
            .len(),
        1
    );
    let audit_id = first_audit_page["items"][0]["id"]
        .as_str()
        .expect("audit id")
        .to_owned();
    let audit_cursor = first_audit_page["next_cursor"]
        .as_str()
        .expect("audit cursor")
        .to_owned();
    assert!(!audit_cursor.contains("offset") && !audit_cursor.contains("page"));

    let audit_event = send(
        &app,
        Method::GET,
        &format!("/api/v1/audit/{audit_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(audit_event.status(), StatusCode::OK);
    assert_eq!(response_json(audit_event).await["id"], audit_id);

    let second_audit_page = send(
        &app,
        Method::GET,
        &format!("/api/v1/audit?limit=1&after={audit_cursor}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(second_audit_page.status(), StatusCode::OK);
    assert_eq!(
        response_json(second_audit_page).await["items"]
            .as_array()
            .expect("audit items")
            .len(),
        1
    );

    let audit_export = send(
        &app,
        Method::GET,
        "/api/v1/audit/export?action=content.created&limit=1",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(audit_export.status(), StatusCode::OK);
    let audit_export = response_json(audit_export).await;
    assert_eq!(audit_export["format"], "mavi.audit.export");
    assert_eq!(audit_export["version"], 1);
    assert_eq!(
        audit_export["items"]
            .as_array()
            .expect("export items")
            .len(),
        1
    );
    assert_eq!(audit_export["truncated"], true);

    let export_audit = send(
        &app,
        Method::GET,
        "/api/v1/audit?action=audit.events.exported&limit=1",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(export_audit.status(), StatusCode::OK);
    assert_eq!(
        response_json(export_audit).await["items"][0]["action"],
        "audit.events.exported"
    );

    let trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/content/{first_content}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(trashed.status(), StatusCode::NO_CONTENT);

    let trash_page = send(
        &app,
        Method::GET,
        "/api/v1/trash?kind=content&limit=1",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(trash_page.status(), StatusCode::OK);
    let trash_page = response_json(trash_page).await;
    assert_eq!(
        trash_page["items"].as_array().expect("trash items").len(),
        1
    );
    assert_eq!(trash_page["items"][0]["id"], first_content);
    let trash_cursor = trash_page["next_cursor"].as_str();
    assert!(trash_cursor.is_none());

    let restored = send(
        &app,
        Method::POST,
        &format!("/api/v1/trash/content/{first_content}/restore"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(restored.status(), StatusCode::NO_CONTENT);
    let restored_read = send(
        &app,
        Method::GET,
        &format!("/api/v1/content/{first_content}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(restored_read.status(), StatusCode::OK);

    let trashed_again = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/content/{first_content}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(trashed_again.status(), StatusCode::NO_CONTENT);
    let permanently_deleted = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/trash/content/{first_content}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(permanently_deleted.status(), StatusCode::NO_CONTENT);
    let missing = send(
        &app,
        Method::GET,
        &format!("/api/v1/content/{first_content}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    let form = send(
        &app,
        Method::POST,
        "/api/v1/forms",
        Some(&owner_token),
        Some(json!({
            "slug": "audit-trash-form",
            "name": "Audit trash form"
        })),
    )
    .await;
    assert_eq!(form.status(), StatusCode::CREATED);
    let form_id = response_json(form).await["id"]
        .as_str()
        .expect("form id")
        .to_owned();
    let form_trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/forms/{form_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(form_trashed.status(), StatusCode::NO_CONTENT);
    let form_trash = send(
        &app,
        Method::GET,
        "/api/v1/trash?kind=form",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(form_trash.status(), StatusCode::OK);
    assert_eq!(response_json(form_trash).await["items"][0]["id"], form_id);
    let form_restored = send(
        &app,
        Method::POST,
        &format!("/api/v1/trash/form/{form_id}/restore"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(form_restored.status(), StatusCode::NO_CONTENT);
    let form_read = send(
        &app,
        Method::GET,
        &format!("/api/v1/forms/{form_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(form_read.status(), StatusCode::OK);
    let form_trashed_again = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/forms/{form_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(form_trashed_again.status(), StatusCode::NO_CONTENT);
    let form_deleted = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/trash/form/{form_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(form_deleted.status(), StatusCode::NO_CONTENT);

    let file = send_raw(
        &app,
        Method::POST,
        "/api/v1/files?name=trash.png",
        Some(&owner_token),
        "application/octet-stream",
        PNG.to_vec(),
    )
    .await;
    assert_eq!(file.status(), StatusCode::CREATED);
    let file_id = response_json(file).await["id"]
        .as_str()
        .expect("file id")
        .to_owned();
    let file_trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/files/{file_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(file_trashed.status(), StatusCode::NO_CONTENT);
    let file_trash = send(
        &app,
        Method::GET,
        "/api/v1/trash?kind=file",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(file_trash.status(), StatusCode::OK);
    assert_eq!(response_json(file_trash).await["items"][0]["id"], file_id);
    let file_deleted = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/trash/file/{file_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(file_deleted.status(), StatusCode::NO_CONTENT);

    let reader_token = create_reader(&app, &owner_token).await;
    let forbidden_audit = send(
        &app,
        Method::GET,
        "/api/v1/audit",
        Some(&reader_token),
        None,
    )
    .await;
    assert_eq!(forbidden_audit.status(), StatusCode::FORBIDDEN);
    let forbidden_trash = send(
        &app,
        Method::GET,
        "/api/v1/trash",
        Some(&reader_token),
        None,
    )
    .await;
    assert_eq!(forbidden_trash.status(), StatusCode::FORBIDDEN);

    let retained_content = send(
        &app,
        Method::GET,
        &format!("/api/v1/content/{second_content}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(retained_content.status(), StatusCode::OK);
}

async fn create_content(app: &Router, token: &str, slug: &str) -> String {
    let response = send(
        app,
        Method::POST,
        "/api/v1/content",
        Some(token),
        Some(json!({
            "kind": "post",
            "language": "en",
            "slug": slug,
            "title": slug
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["id"]
        .as_str()
        .expect("content id")
        .to_owned()
}

async fn create_reader(app: &Router, owner_token: &str) -> String {
    let role = send(
        app,
        Method::POST,
        "/api/v1/roles",
        Some(owner_token),
        Some(json!({
            "name": "audit-trash-reader",
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
            "email": "audit-trash-reader@example.com",
            "name": "Audit trash reader",
            "password": "long-enough-password",
            "role_ids": [role_id]
        })),
    )
    .await;
    assert_eq!(person.status(), StatusCode::CREATED);
    support::verify_email(
        app,
        &response_json(person).await,
        "audit-trash-reader@example.com",
    )
    .await;
    support::login(app, "audit-trash-reader@example.com").await
}
