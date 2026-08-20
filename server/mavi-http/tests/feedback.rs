use axum::{http::Method, http::StatusCode};
use serde_json::json;

mod support;
use support::{bootstrap, response_json, send};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn feedback_reports_are_canonical_cursor_listable_and_audited() {
    let (app, database, site_id) = support::build_app_with_database().await;
    let owner_token = bootstrap(&app, "Feedback test").await;

    let first = send(
        &app,
        Method::POST,
        "/api/v1/feedback/reports",
        Some(&owner_token),
        Some(json!({
            "kind": "broken",
            "title": "The inbox is empty",
            "body": "A submitted form is not visible.",
            "context": {"screen": "/dashboard/forms", "browser": "test"}
        })),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first = response_json(first).await;
    assert_eq!(first["state"], "open");
    assert_eq!(first["context"]["screen"], "/dashboard/forms");

    let second = send(
        &app,
        Method::POST,
        "/api/v1/feedback/reports",
        Some(&owner_token),
        Some(json!({"kind": "wanted", "title": "A better search"})),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);

    let page = send(
        &app,
        Method::GET,
        "/api/v1/feedback/reports?limit=1",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(page.status(), StatusCode::OK);
    let page = response_json(page).await;
    assert_eq!(page["items"].as_array().expect("items").len(), 1);
    let cursor = page["next_cursor"].as_str().expect("next cursor");
    assert!(!cursor.contains("offset") && !cursor.contains("page"));

    let next = send(
        &app,
        Method::GET,
        &format!("/api/v1/feedback/reports?after={cursor}&limit=1"),
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

    let invalid = send(
        &app,
        Method::POST,
        "/api/v1/feedback/reports",
        Some(&owner_token),
        Some(json!({"kind": "broken", "title": "invalid", "unexpected": true})),
    )
    .await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let context = mavi_core::SiteContext::public(site_id);
    let mut transaction = database.begin(&context).await.expect("audit scope");
    let count: i64 = sqlx::query_scalar(
        "select count(*) from audit_events where site_id = $1 and action = 'feedback.report.created'",
    )
    .bind(site_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("feedback audit count");
    assert_eq!(count, 2);
    transaction.commit().await.expect("audit commit");
}
