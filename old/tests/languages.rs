//! Which languages a site writes in.

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
async fn the_first_language_a_site_adds_is_the_one_it_writes_in() {
    let site = Site::new().await;

    let (status, added) = site
        .send(
            "POST",
            "/api/languages",
            // Not asked for, and true anyway: a site with a language and no
            // default is a site where nothing can be written.
            Some(serde_json::json!({ "code": "en", "name": "English" })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{added}");
    assert_eq!(added["is_default"], true);
}

#[tokio::test]
async fn a_second_default_takes_over_from_the_first() {
    let site = Site::new().await;

    site.send(
        "POST",
        "/api/languages",
        Some(serde_json::json!({ "code": "en", "name": "English" })),
    )
    .await;

    site.send(
        "POST",
        "/api/languages",
        Some(serde_json::json!({ "code": "tr", "name": "Türkçe", "is_default": true })),
    )
    .await;

    let (_, listed) = site.send("GET", "/api/languages", None).await;

    let defaults: Vec<&str> = listed
        .as_array()
        .expect("a list")
        .iter()
        .filter(|language| language["is_default"] == true)
        .filter_map(|language| language["code"].as_str())
        .collect();

    assert_eq!(defaults, vec!["tr"], "{listed}");
}

#[tokio::test]
async fn a_site_cannot_be_left_with_no_default() {
    let site = Site::new().await;

    site.send(
        "POST",
        "/api/languages",
        Some(serde_json::json!({ "code": "en", "name": "English" })),
    )
    .await;

    let (status, refused) = site
        .send(
            "PATCH",
            "/api/languages/en",
            Some(serde_json::json!({ "is_default": false })),
        )
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");

    // And what was refused did not happen.
    let (_, listed) = site.send("GET", "/api/languages", None).await;

    assert_eq!(listed[0]["is_default"], true);
}

#[tokio::test]
async fn a_language_something_is_written_in_is_not_taken_away() {
    let site = Site::new().await;

    site.send(
        "POST",
        "/api/languages",
        Some(serde_json::json!({ "code": "en", "name": "English" })),
    )
    .await;

    site.send(
        "POST",
        "/api/languages",
        Some(serde_json::json!({ "code": "tr", "name": "Türkçe" })),
    )
    .await;

    site.send(
        "POST",
        "/api/posts",
        Some(serde_json::json!({ "language": "tr", "title": "Bir yazı" })),
    )
    .await;

    let (status, refused) = site.send("DELETE", "/api/languages/tr", None).await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
    assert_eq!(refused["error"]["named"]["posts"], "1");
}

#[tokio::test]
async fn a_language_nothing_is_written_in_goes() {
    let site = Site::new().await;

    site.send(
        "POST",
        "/api/languages",
        Some(serde_json::json!({ "code": "en", "name": "English" })),
    )
    .await;

    site.send(
        "POST",
        "/api/languages",
        Some(serde_json::json!({ "code": "tr", "name": "Türkçe" })),
    )
    .await;

    let (status, _) = site.send("DELETE", "/api/languages/tr", None).await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, listed) = site.send("GET", "/api/languages", None).await;

    assert_eq!(listed.as_array().expect("a list").len(), 1);
}

#[tokio::test]
async fn something_that_is_not_a_language_is_refused_in_words() {
    let site = Site::new().await;

    let (status, refused) = site
        .send(
            "POST",
            "/api/languages",
            Some(serde_json::json!({ "code": "english", "name": "English" })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(
        refused["error"]["key"], "a_language_is_two_letters_and_a_place",
        "the refusal was a constraint name rather than a sentence"
    );
}

#[tokio::test]
async fn the_same_language_twice_is_a_conflict() {
    let site = Site::new().await;

    for _ in 0..2 {
        site.send(
            "POST",
            "/api/languages",
            Some(serde_json::json!({ "code": "en", "name": "English" })),
        )
        .await;
    }

    let (status, _) = site
        .send(
            "POST",
            "/api/languages",
            Some(serde_json::json!({ "code": "en", "name": "English" })),
        )
        .await;

    assert_eq!(status, StatusCode::CONFLICT);
}
