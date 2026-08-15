//! Who did what, and when.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::{Access, Capability, Needs, every_grant};
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::harness;
use mavi::testing::{a_role, a_user};

const PASSWORD: &str = "a long enough password";

struct Site {
    db: Db,
    router: axum::Router,
    host: String,
    token: String,
}

impl Site {
    async fn new() -> Self {
        Self::where_somebody_can(&every_grant()).await
    }

    async fn where_somebody_can(grants: &[String]) -> Self {
        let db = harness().await;
        let host = format!("{}.example", Uuid::now_v7().simple());
        let role = a_role(&db, "somebody", grants).await;
        let (_, email) = a_user(&db, role, PASSWORD).await;

        let mut conn = db.begin().await.expect("begin");
        sqlx::query(
            "insert into languages (code, name, is_default)
             values ('en', 'English', true)",
        )
        .execute(conn.conn())
        .await
        .expect("a language");
        conn.commit().await.expect("commit");

        let site = Self {
            db: db.clone(),
            router: mavi::router(AppState::new(db)),
            host,
            token: String::new(),
        };

        let (status, body) = site
            .send(
                "POST",
                "/api/auth/session",
                Some(serde_json::json!({ "email": email, "password": PASSWORD })),
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{body}");

        Self {
            token: body["token"].as_str().expect("a token").to_owned(),
            ..site
        }
    }

    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, &self.host);

        if !self.token.is_empty() {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", self.token));
        }

        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .expect("a request");

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("a response");

        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();

        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }
}

#[tokio::test]
async fn what_somebody_did_is_readable_afterwards() {
    let site = Site::new().await;

    let (status, written) = site
        .send(
            "POST",
            "/api/posts",
            Some(serde_json::json!({ "language": "en", "title": "Something written" })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{written}");

    let (status, log) = site.send("GET", "/api/audit", None).await;

    assert_eq!(status, StatusCode::OK, "{log}");

    let wrote = log["items"]
        .as_array()
        .expect("a page")
        .iter()
        .find(|entry| entry["subject"] == "post")
        .expect("what was written");

    assert_eq!(wrote["action"], "wrote");
    assert_eq!(wrote["after"]["title"], "Something written");
    assert!(
        wrote["actor_name"].is_string(),
        "the log says who, and the name is what a screen shows: {wrote}"
    );
}

#[tokio::test]
async fn one_thing_s_history_is_one_question() {
    let site = Site::new().await;

    let (_, written) = site
        .send(
            "POST",
            "/api/posts",
            Some(serde_json::json!({ "language": "en", "title": "First" })),
        )
        .await;

    let id = written["id"].as_str().expect("an id").to_owned();

    site.send(
        "PATCH",
        &format!("/api/posts/{id}"),
        Some(serde_json::json!({ "title": "Second" })),
    )
    .await;

    let (_, log) = site
        .send("GET", &format!("/api/audit?subject_id={id}"), None)
        .await;

    let actions: Vec<&str> = log["items"]
        .as_array()
        .expect("a page")
        .iter()
        .filter_map(|entry| entry["action"].as_str())
        .collect();

    assert_eq!(actions, vec!["changed", "wrote"], "{log}");
}

#[tokio::test]
async fn reading_the_log_asks_for_the_grant_that_is_for_it() {
    // Everything but the audit: somebody who can write posts is not somebody
    // who can read who else has.
    let grants: Vec<String> = every_grant()
        .into_iter()
        .filter(|grant| !grant.starts_with("audit:"))
        .collect();

    let site = Site::where_somebody_can(&grants).await;

    let (status, _) = site.send("GET", "/api/audit", None).await;

    assert_eq!(status, StatusCode::FORBIDDEN);

    // And the grant is the one that says so.
    let _ = Needs::new(Capability::Audit, Access::View);
}

#[tokio::test]
async fn the_log_can_be_taken_away_as_a_file() {
    let site = Site::new().await;

    site.send(
        "POST",
        "/api/posts",
        Some(serde_json::json!({ "language": "en", "title": "Something Written" })),
    )
    .await;

    let response = site
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/audit/export")
                .header(header::HOST, &site.host)
                .header(header::AUTHORIZATION, format!("Bearer {}", site.token))
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");

    assert_eq!(response.status(), StatusCode::OK);

    let kind = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();

    assert!(kind.starts_with("text/csv"), "{kind}");

    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();

    let file = String::from_utf8_lossy(&bytes);

    assert!(file.starts_with("when,who,kind,action"), "{file}");

    // What changed is a site's own content and does not go in a file that
    // leaves the machine.
    assert!(
        !file.contains("Something Written"),
        "the export carried what a post says: {file}"
    );
}

#[tokio::test]
async fn the_log_can_be_read_for_one_kind_of_doing() {
    let site = Site::new().await;

    site.send(
        "POST",
        "/api/posts",
        Some(serde_json::json!({ "language": "en", "title": "One" })),
    )
    .await;

    let (status, only) = site.send("GET", "/api/audit?action=wrote", None).await;

    assert_eq!(status, StatusCode::OK, "{only}");

    let actions: Vec<&str> = only["items"]
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|entry| entry["action"].as_str())
        .collect();

    assert!(!actions.is_empty(), "{only}");
    assert!(
        actions.iter().all(|action| action.starts_with("wrote")),
        "a filter on one kind of doing answered with others: {actions:?}"
    );
}

#[tokio::test]
async fn a_name_that_looks_like_a_formula_is_not_one() {
    let site = Site::new().await;

    // A person's name is their own, and a spreadsheet runs what starts like
    // this unless something stops it.
    let mut conn = site.db.begin().await.expect("begin");

    sqlx::query("update users set name = '=1+1' where id is not null")
        .execute(conn.conn())
        .await
        .expect("a name");

    conn.commit().await.expect("commit");

    site.send(
        "POST",
        "/api/posts",
        Some(serde_json::json!({ "language": "en", "title": "Anything" })),
    )
    .await;

    let response = site
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/audit/export")
                .header(header::HOST, &site.host)
                .header(header::AUTHORIZATION, format!("Bearer {}", site.token))
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");

    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();

    let file = String::from_utf8_lossy(&bytes);

    assert!(file.contains("\"'=1+1\""), "{file}");
    assert!(
        !file.contains("\"=1+1\""),
        "a name a spreadsheet would run went out as it was written: {file}"
    );
}
