//! A key an assistant can work with, and the shorter list of what it may do.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::harness;
use mavi::testing::{a_role, a_user};

const PASSWORD: &str = "a long enough password";

struct Site {
    router: axum::Router,
    host: String,
    token: String,
    #[expect(dead_code, reason = "kept so the site outlives the leased database")]
    db: Db,
}

impl Site {
    async fn new() -> Self {
        let db = harness().await;
        let host = format!("{}.example", Uuid::now_v7().simple());
        let role = a_role(&db, "owner", &every_grant()).await;
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
            router: mavi::router(AppState::new(db.clone())),
            host,
            token: String::new(),
            db,
        };

        let (status, body) = site
            .send(
                "POST",
                "/api/auth/session",
                None,
                Some(serde_json::json!({ "email": email, "password": PASSWORD })),
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{body}");

        Self {
            token: body["token"].as_str().expect("a token").to_owned(),
            ..site
        }
    }

    async fn a_key(&self) -> serde_json::Value {
        let (status, handed) = self
            .send("POST", "/api/assistant/handover", Some(&self.token), None)
            .await;

        assert_eq!(status, StatusCode::CREATED, "{handed}");
        handed
    }

    async fn send(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, &self.host);

        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
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
async fn the_key_it_is_handed_actually_writes() {
    let site = Site::new().await;
    let handed = site.a_key().await;
    let key = handed["token"].as_str().expect("a key").to_owned();

    let (status, written) = site
        .send(
            "POST",
            "/api/posts",
            Some(&key),
            Some(serde_json::json!({ "language": "en", "title": "Written by an assistant" })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::CREATED,
        "the key a site handed over could not use it: {written}"
    );
}

#[tokio::test]
async fn what_it_may_not_do_is_refused_rather_than_promised() {
    let site = Site::new().await;
    let key = site.a_key().await["token"]
        .as_str()
        .expect("a key")
        .to_owned();

    // The settings hold other services' credentials.
    let (status, _) = site.send("GET", "/api/plugins", Some(&key), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Accounts are how somebody gets in, including this one.
    let (status, _) = site.send("GET", "/api/people", Some(&key), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // And nothing goes for good.
    let (status, _) = site
        .send(
            "DELETE",
            &format!("/api/trash/posts/{}", Uuid::now_v7()),
            Some(&key),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_key_is_listed_by_something_that_is_not_the_key() {
    let site = Site::new().await;
    let handed = site.a_key().await;

    let (status, listed) = site
        .send("GET", "/api/assistant/keys", Some(&site.token), None)
        .await;

    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["items"][0]["id"], handed["id"]);
    assert!(
        !listed
            .to_string()
            .contains(handed["token"].as_str().expect("a key")),
        "the listing gave the key away: {listed}"
    );
}

#[tokio::test]
async fn a_key_can_be_taken_back_before_it_runs_out() {
    let site = Site::new().await;
    let handed = site.a_key().await;
    let key = handed["token"].as_str().expect("a key").to_owned();
    let id = handed["id"].as_str().expect("an id").to_owned();

    let (status, _) = site
        .send(
            "DELETE",
            &format!("/api/assistant/keys/{id}"),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = site.send("GET", "/api/posts", Some(&key), None).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a key that was taken back still works"
    );
}

#[tokio::test]
async fn taking_a_key_back_does_not_end_anybody_else_s_session() {
    let site = Site::new().await;
    let handed = site.a_key().await;
    let id = handed["id"].as_str().expect("an id").to_owned();

    site.send(
        "DELETE",
        &format!("/api/assistant/keys/{id}"),
        Some(&site.token),
        None,
    )
    .await;

    let (status, _) = site
        .send("GET", "/api/auth/me", Some(&site.token), None)
        .await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_second_key_does_not_end_the_first() {
    let site = Site::new().await;
    let one = site.a_key().await["token"]
        .as_str()
        .expect("a key")
        .to_owned();

    site.a_key().await;

    let (status, _) = site.send("GET", "/api/posts", Some(&one), None).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "handing out a second key ended the first, which nobody asked for"
    );
}
