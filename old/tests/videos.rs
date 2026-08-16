//! A video, from the file that was uploaded to the thing that plays.

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

/// The first bytes of an MP4, which is what decides what a file is here.
const AN_MP4: &[u8] = b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00mp42isom";

struct Site {
    db: Db,
    router: axum::Router,
    host: String,
    token: String,
}

impl Site {
    async fn new() -> Self {
        let db = harness().await;
        let host = format!("{}.example", Uuid::now_v7().simple());
        let role = a_role(&db, "owner", &every_grant()).await;
        let (_, email) = a_user(&db, role, PASSWORD).await;

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

    /// A file in the library, put there the way an upload would.
    async fn a_file(&self, mime: &str) -> Uuid {
        let mut conn = self.db.begin().await.expect("begin");

        let row: (Uuid,) = sqlx::query_as(
            "insert into media (original_name, mime, bytes, location, checksum)
             values ('a-film.mp4', $1, $2, 'nowhere', decode('00', 'hex'))
             returning id",
        )
        .bind(mime)
        .bind(i64::try_from(AN_MP4.len()).expect("a size"))
        .fetch_one(conn.conn())
        .await
        .expect("a file");

        conn.commit().await.expect("commit");

        row.0
    }

    async fn works(&self) {
        let state = AppState::new(self.db.clone());

        for _ in 0..4 {
            mavi::jobs::tick(&state, "test").await.expect("tick");
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
async fn a_video_is_waiting_until_something_has_looked_at_it() {
    let site = Site::new().await;
    let file = site.a_file("video/mp4").await;

    let (status, added) = site
        .send(
            "POST",
            "/api/videos",
            Some(serde_json::json!({ "media_id": file, "title": "A film" })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{added}");
    assert_eq!(added["state"], "waiting");

    let id = added["id"].as_str().expect("an id").to_owned();

    site.works().await;

    let (_, after) = site.send("GET", &format!("/api/videos/{id}"), None).await;

    // Nothing is configured to transcode in a test, and a video left saying
    // "working" for ever on a machine that will never work on it is worse than
    // one that plays the file it was given.
    assert_eq!(after["state"], "ready", "{after}");
    assert!(
        after["plays"]["as_uploaded"].is_string(),
        "nothing said where it plays: {after}"
    );
}

#[tokio::test]
async fn a_file_that_is_not_a_video_is_not_one() {
    let site = Site::new().await;
    let file = site.a_file("image/png").await;

    let (status, refused) = site
        .send(
            "POST",
            "/api/videos",
            Some(serde_json::json!({ "media_id": file, "title": "A film" })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}

#[tokio::test]
async fn a_video_of_a_file_that_is_not_there_is_refused() {
    let site = Site::new().await;

    let (status, _) = site
        .send(
            "POST",
            "/api/videos",
            Some(serde_json::json!({ "media_id": Uuid::now_v7(), "title": "A film" })),
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn nothing_unsigned_says_a_video_is_ready() {
    let site = Site::new().await;

    let (status, refused) = site
        .send(
            "POST",
            "/api/sites/videos/callback",
            Some(serde_json::json!({ "reference": "anything", "ok": true })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "anybody could say a video was ready: {refused}"
    );
}

#[tokio::test]
async fn a_video_is_listed_and_thrown_away_like_everything_else() {
    let site = Site::new().await;
    let file = site.a_file("video/mp4").await;

    let (_, added) = site
        .send(
            "POST",
            "/api/videos",
            Some(serde_json::json!({ "media_id": file, "title": "A film" })),
        )
        .await;

    let id = added["id"].as_str().expect("an id").to_owned();

    let (status, listed) = site.send("GET", "/api/videos", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["items"][0]["title"], "A film");

    let (status, _) = site
        .send("DELETE", &format!("/api/videos/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = site.send("GET", &format!("/api/videos/{id}"), None).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}
