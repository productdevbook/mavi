//! What a visitor sees on a site's own address.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use mavi::kernel::storage::{LocalDisk, Store};
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
    kept_in: std::path::PathBuf,
}

impl Site {
    async fn new() -> Self {
        let db = harness().await;
        let host = format!("{}.example", Uuid::now_v7().simple());
        let role = a_role(&db, "owner", &every_grant()).await;
        let (_, email) = a_user(&db, role, PASSWORD).await;

        let kept_in = std::env::temp_dir().join(format!("mavi-pages-{}", Uuid::now_v7().simple()));

        let mut state = AppState::new(db.clone());
        state.store = std::sync::Arc::new(Store::Disk(LocalDisk::at(&kept_in)));

        let site = Self {
            db,
            router: mavi::router(state),
            host,
            token: String::new(),
            kept_in,
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

    /// Writes a page into the theme and publishes, the way the panel would.
    async fn publishes(&self, path: &str, body: &str) -> String {
        let (status, written) = self
            .send(
                "PUT",
                "/api/design/files",
                Some(serde_json::json!({ "path": path, "body": body })),
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{written}");

        let (status, publish) = self.send("POST", "/api/design/publishes", None).await;

        assert_eq!(status, StatusCode::ACCEPTED, "{publish}");

        let state = {
            let mut state = AppState::new(self.db.clone());
            state.store = std::sync::Arc::new(Store::Disk(LocalDisk::at(&self.kept_in)));
            state
        };

        for _ in 0..4 {
            mavi::jobs::tick(&state, "test").await.expect("tick");
        }

        publish["id"].as_str().expect("an id").to_owned()
    }

    async fn visit(&self, path: &str) -> (StatusCode, String, Option<String>) {
        let request = Request::builder()
            .method("GET")
            .uri(path)
            .header(header::HOST, &self.host)
            .body(Body::empty())
            .expect("a request");

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("a response");

        let status = response.status();

        let kind = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();

        (status, String::from_utf8_lossy(&bytes).into_owned(), kind)
    }

    async fn writes_in(&self, code: &str) {
        let mut conn = self.db.begin().await.expect("begin");

        sqlx::query(
            "insert into languages (code, name, is_default)
             values ($1, $1, true)",
        )
        .bind(code)
        .execute(conn.conn())
        .await
        .expect("a language");

        conn.commit().await.expect("commit");
    }

    /// Where an address sends a visitor, when it sends them anywhere.
    async fn where_to(&self, path: &str) -> (StatusCode, Option<String>) {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(path)
                    .header(header::HOST, &self.host)
                    .body(Body::empty())
                    .expect("a request"),
            )
            .await
            .expect("a response");

        let to = response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        (response.status(), to)
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
async fn what_was_published_is_what_a_visitor_gets() {
    let site = Site::new().await;
    site.publishes("public/index.html", "<h1>A site</h1>").await;

    let (status, body, kind) = site.visit("/").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, "<h1>A site</h1>");
    assert_eq!(kind.as_deref(), Some("text/html; charset=utf-8"));
}

#[tokio::test]
async fn a_folder_is_its_index() {
    let site = Site::new().await;
    site.publishes("public/about/index.html", "<h1>About</h1>")
        .await;

    let (status, body, _) = site.visit("/about").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "<h1>About</h1>");
}

#[tokio::test]
async fn a_site_that_has_never_published_has_nothing_to_show() {
    let site = Site::new().await;

    let (status, _, _) = site.visit("/").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_page_that_is_not_there_is_the_site_s_own_404() {
    let site = Site::new().await;
    site.publishes("public/404.html", "<h1>Nothing here</h1>")
        .await;

    let (status, body, _) = site.visit("/something-else").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, "<h1>Nothing here</h1>");
}

#[tokio::test]
async fn an_address_a_page_used_to_answer_on_says_where_it_went() {
    let site = Site::new().await;
    site.publishes("public/404.html", "<h1>Nothing here</h1>")
        .await;
    site.writes_in("en").await;

    let (status, post) = site
        .send(
            "POST",
            "/api/posts",
            Some(serde_json::json!({ "language": "en", "title": "Where we are" })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{post}");

    let id = post["id"].as_str().expect("an id");
    let (status, renamed) = site
        .send(
            "PATCH",
            &format!("/api/posts/{id}"),
            Some(serde_json::json!({ "slug": "where-to-find-us" })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{renamed}");

    let (status, to) = site.where_to("/blog/where-we-are").await;

    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(to.as_deref(), Some("/blog/where-to-find-us"));
}

#[tokio::test]
async fn a_site_s_source_is_not_on_its_own_address() {
    let site = Site::new().await;
    site.publishes("src/secrets.js", "// what builds the site")
        .await;

    let (status, body, _) = site.visit("/secrets.js").await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a site's source was served on its own address: {body}"
    );
}

#[tokio::test]
async fn the_api_still_answers_where_a_page_would() {
    let site = Site::new().await;
    site.publishes("public/index.html", "<h1>A site</h1>").await;

    let (status, _) = site.send("GET", "/api/auth/me", None).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "the pages took an address the API answers on"
    );
}

#[tokio::test]
async fn nothing_climbs_out_of_what_was_published() {
    let site = Site::new().await;
    site.publishes("public/index.html", "<h1>A site</h1>").await;

    for asked in [
        "/../../etc/passwd",
        "/assets/../../../index.html",
        "/%2e%2e/%2e%2e/etc/passwd",
    ] {
        let (status, body, _) = site.visit(asked).await;

        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{asked} reached something: {body}"
        );
    }
}
