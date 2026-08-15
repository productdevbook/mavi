//! What is being served changes when a publish says so, and not when somebody
//! saves a file.

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

struct Site {
    db: Db,
    router: axum::Router,
    host: String,
    token: String,
}

async fn a_site() -> Site {
    let db = harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());
    let role = a_role(&db, "owner", &every_grant()).await;
    let password = "a long enough password";
    let (_, email) = a_user(&db, role, password).await;

    let router = mavi::router(AppState::new(db.clone()));

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/session")
                .header(header::HOST, &host)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "email": email, "password": password }).to_string(),
                ))
                .expect("a request"),
        )
        .await
        .expect("a response");

    let body: serde_json::Value = serde_json::from_slice(
        &response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes(),
    )
    .expect("json");

    Site {
        db,
        router,
        host,
        token: body["token"].as_str().expect("a token").to_owned(),
    }
}

impl Site {
    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, &self.host)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.token));

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
async fn what_is_served_changes_when_a_publish_says_so() {
    let site = a_site().await;

    let (status, body) = site
        .send(
            "PUT",
            "/api/design/files",
            Some(serde_json::json!({
                "path": "src/pages/index.astro", "body": "<h1>Hello</h1>"
            })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");

    // Nothing is live yet.
    let (_, live) = site
        .send("GET", "/api/design/files?branch=live", None)
        .await;
    assert_eq!(live.as_array().expect("a list").len(), 0);

    let (status, publish) = site.send("POST", "/api/design/publishes", None).await;

    assert_eq!(status, StatusCode::ACCEPTED, "{publish}");

    let state = AppState::new(site.db.clone());
    mavi::jobs::tick(&state, "test").await.expect("tick");

    let (_, live) = site
        .send("GET", "/api/design/files?branch=live", None)
        .await;

    assert_eq!(live.as_array().expect("a list").len(), 1);
    assert_eq!(live[0]["path"], "src/pages/index.astro");
}

#[tokio::test]
async fn nothing_writes_straight_to_what_is_being_served() {
    let site = a_site().await;

    let (status, _) = site
        .send(
            "PUT",
            "/api/design/files",
            Some(serde_json::json!({
                "path": "src/pages/index.astro", "body": "<h1>Hello</h1>", "branch": "live"
            })),
        )
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn what_decides_how_a_site_is_built_is_not_a_thing_a_site_edits() {
    let site = a_site().await;

    for path in [
        "package.json",
        "astro.config.mjs",
        "../etc/passwd",
        "src/../../elsewhere",
    ] {
        let (status, _) = site
            .send(
                "PUT",
                "/api/design/files",
                Some(serde_json::json!({ "path": path, "body": "anything" })),
            )
            .await;

        assert!(
            status == StatusCode::FORBIDDEN || status == StatusCode::UNPROCESSABLE_ENTITY,
            "{path} was writable"
        );
    }
}

#[tokio::test]
async fn one_publish_at_a_time_per_site() {
    let site = a_site().await;

    site.send(
        "PUT",
        "/api/design/files",
        Some(serde_json::json!({ "path": "src/x.astro", "body": "x" })),
    )
    .await;

    let (first, _) = site.send("POST", "/api/design/publishes", None).await;
    let (again, _) = site.send("POST", "/api/design/publishes", None).await;

    assert_eq!(first, StatusCode::ACCEPTED);
    assert_eq!(
        again,
        StatusCode::CONFLICT,
        "two builds of one site at once"
    );
}

#[tokio::test]
async fn a_publish_is_the_whole_of_a_site_rather_than_a_patch() {
    let site = a_site().await;

    for path in ["src/one.astro", "src/two.astro"] {
        site.send(
            "PUT",
            "/api/design/files",
            Some(serde_json::json!({ "path": path, "body": "x" })),
        )
        .await;
    }

    site.send("POST", "/api/design/publishes", None).await;

    let state = AppState::new(site.db.clone());
    mavi::jobs::tick(&state, "test").await.expect("tick");

    // One goes away on the branch, and the next publish takes it off the site.
    let mut conn = site.db.begin().await.expect("begin");
    sqlx::query("delete from theme_files where branch = 'draft' and path = 'src/two.astro'")
        .execute(conn.conn())
        .await
        .expect("removed");
    conn.commit().await.expect("commit");

    site.send("POST", "/api/design/publishes", None).await;
    mavi::jobs::tick(&state, "test").await.expect("tick");

    let (_, live) = site
        .send("GET", "/api/design/files?branch=live", None)
        .await;

    assert_eq!(live.as_array().expect("a list").len(), 1);
    assert_eq!(live[0]["path"], "src/one.astro");
}

/// A builder that answers on a socket, and can be told to fail.
mod builder {
    use axum::routing::post;
    use axum::{Json, Router};

    pub async fn that(ok: bool) -> String {
        let app = Router::new().route(
            "/builds",
            post(move |Json(_): Json<serde_json::Value>| async move {
                Json(serde_json::json!({
                    "ok": ok,
                    "log": if ok { "built" } else { "src/x.astro: unexpected token" },
                }))
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a socket");
        let address = listener.local_addr().expect("an address");

        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        format!("http://{address}")
    }
}

fn building_at(at: &str) -> mavi::kernel::builder::Builder {
    use mavi::kernel::builder::{Builder, Elsewhere};
    use mavi::kernel::secret::Secret;

    Builder::Elsewhere(Elsewhere {
        at: at.to_owned(),
        key: Secret::new("a key".to_owned()),
    })
}

#[tokio::test]
async fn a_build_that_fails_leaves_what_is_live_alone() {
    let site = a_site().await;

    site.send(
        "PUT",
        "/api/design/files",
        Some(serde_json::json!({ "path": "src/one.astro", "body": "x" })),
    )
    .await;

    // One good publish, so there is something live to leave alone.
    site.send("POST", "/api/design/publishes", None).await;

    let mut state = AppState::new(site.db.clone());
    state.builder = std::sync::Arc::new(building_at(&builder::that(true).await));

    mavi::jobs::tick(&state, "test").await.expect("tick");

    site.send(
        "PUT",
        "/api/design/files",
        Some(serde_json::json!({ "path": "src/two.astro", "body": "broken" })),
    )
    .await;

    site.send("POST", "/api/design/publishes", None).await;

    let mut state = AppState::new(site.db.clone());
    state.builder = std::sync::Arc::new(building_at(&builder::that(false).await));

    mavi::jobs::tick(&state, "test").await.expect("tick");

    let (_, live) = site
        .send("GET", "/api/design/files?branch=live", None)
        .await;

    assert_eq!(
        live.as_array().expect("a list").len(),
        1,
        "a failed build changed what is being served"
    );

    let (_, history) = site.send("GET", "/api/design/publishes", None).await;

    assert_eq!(history["items"][0]["state"], "failed");

    let mut conn = site.db.begin().await.expect("begin");

    let log: (Option<String>,) = sqlx::query_as("select log from publishes where state = 'failed'")
        .fetch_one(conn.conn())
        .await
        .expect("a publish");

    assert!(
        log.0.is_some_and(|log| log.contains("unexpected token")),
        "a failed build has to say what went wrong"
    );
}

#[tokio::test]
async fn a_publish_can_be_told_not_to() {
    let site = a_site().await;

    site.send(
        "PUT",
        "/api/design/files",
        Some(serde_json::json!({ "path": "src/x.astro", "body": "x" })),
    )
    .await;

    let (_, publish) = site.send("POST", "/api/design/publishes", None).await;
    let id = publish["id"].as_str().expect("an id");

    let (status, cancelled) = site
        .send("POST", &format!("/api/design/publishes/{id}/cancel"), None)
        .await;

    assert_eq!(status, StatusCode::OK, "{cancelled}");
    assert_eq!(cancelled["state"], "cancelled");

    // The work is still in the queue; running it does nothing, because the
    // publish it was for is not wanted.
    let state = AppState::new(site.db.clone());
    mavi::jobs::tick(&state, "test").await.expect("tick");

    let (_, live) = site
        .send("GET", "/api/design/files?branch=live", None)
        .await;

    assert_eq!(
        live.as_array().expect("a list").len(),
        0,
        "a cancelled publish went live anyway"
    );

    // And another one can start, because the cancelled one is not in the way.
    let (status, _) = site.send("POST", "/api/design/publishes", None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn a_publish_that_finished_cannot_be_cancelled() {
    let site = a_site().await;

    site.send(
        "PUT",
        "/api/design/files",
        Some(serde_json::json!({ "path": "src/x.astro", "body": "x" })),
    )
    .await;

    let (_, publish) = site.send("POST", "/api/design/publishes", None).await;
    let id = publish["id"].as_str().expect("an id");

    let state = AppState::new(site.db.clone());
    mavi::jobs::tick(&state, "test").await.expect("tick");

    let (status, _) = site
        .send("POST", &format!("/api/design/publishes/{id}/cancel"), None)
        .await;

    assert_eq!(status, StatusCode::CONFLICT);
}

impl Site {
    /// A page as a visitor gets it, with nothing said about who is asking.
    async fn visit(&self, path: &str) -> (StatusCode, String) {
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

        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();

        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    async fn wrote(&self, path: &str, body: &str) {
        let (status, said) = self
            .send(
                "PUT",
                "/api/design/files",
                Some(serde_json::json!({ "path": path, "body": body })),
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{said}");
    }

    async fn built(&self) {
        let state = AppState::new(self.db.clone());
        mavi::jobs::tick(&state, "test").await.expect("tick");
    }
}

#[tokio::test]
async fn looking_at_a_design_leaves_what_is_served_alone() {
    let site = a_site().await;

    site.wrote("public/index.html", "the published one").await;
    site.send("POST", "/api/design/publishes", None).await;
    site.built().await;

    site.wrote("public/index.html", "the one nobody approved")
        .await;

    let (status, asked) = site.send("POST", "/api/design/previews", None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{asked}");
    site.built().await;

    let (_, design) = site.send("GET", "/api/design", None).await;
    let at = design["preview_at"].as_str().expect("somewhere to look");

    let (status, looked) = site.visit(at).await;
    assert_eq!(status, StatusCode::OK, "{looked}");
    assert_eq!(looked, "the one nobody approved");

    let (status, served) = site.visit("/").await;
    assert_eq!(status, StatusCode::OK, "{served}");
    assert_eq!(
        served, "the published one",
        "looking at a design published it"
    );
}

#[tokio::test]
async fn what_a_publish_would_change_is_said_before_it_happens() {
    let site = a_site().await;

    site.wrote("public/index.html", "one").await;
    site.wrote("public/gone.html", "going").await;
    site.send("POST", "/api/design/publishes", None).await;
    site.built().await;

    let (_, settled) = site.send("GET", "/api/design", None).await;
    assert_eq!(
        settled["changed"].as_array().expect("a list").len(),
        0,
        "a site with nothing to publish said it had something: {settled}"
    );

    site.wrote("public/index.html", "two").await;
    site.wrote("public/new.html", "new").await;

    let (status, design) = site.send("GET", "/api/design", None).await;

    assert_eq!(status, StatusCode::OK, "{design}");

    let mut changed: Vec<(&str, &str)> = design["changed"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|change| {
            (
                change["path"].as_str().unwrap_or_default(),
                change["kind"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    changed.sort_unstable();

    assert_eq!(
        changed,
        vec![
            ("public/index.html", "changed"),
            ("public/new.html", "added")
        ],
        "{design}"
    );
}

/// A preview answers under the id of the publish that made it and nothing
/// links to it, so the only way to reach one is to be told where it is. An id
/// that is not a publish is not somewhere to look.
#[tokio::test]
async fn a_preview_nothing_built_is_not_reachable() {
    let site = a_site().await;
    site.wrote("public/index.html", "the real one").await;
    site.send("POST", "/api/design/previews", None).await;
    site.built().await;

    let (status, said) = site.visit(&mavi::edge::preview_path(Uuid::now_v7())).await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{said}");
}

#[tokio::test]
async fn one_design_file_can_be_read_back() {
    let site = a_site().await;

    site.wrote("src/pages/index.astro", "the whole of it").await;

    let (status, read) = site
        .send("GET", "/api/design/file?path=src/pages/index.astro", None)
        .await;

    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(read["body"], "the whole of it");
    assert_eq!(read["branch"], "draft");

    let (status, _) = site
        .send("GET", "/api/design/file?path=src/nothing.astro", None)
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A site whose pages are built, published the way a machine with a generator
/// publishes them — the whole chain, rather than each link on its own.
///
/// What was untested until this: a theme goes in, a command builds it in a
/// workspace of its own, what it produced goes to the store under the id of
/// the publish that made it, and a visitor on the site's own address is served
/// that. Every part of that had a test; the line through them did not, and it
/// is the line that a machine actually runs.
#[tokio::test]
async fn a_site_that_has_to_be_built_is_built_and_then_served() {
    use mavi::building::Generator;
    use mavi::kernel::builder::Builder;
    use mavi::kernel::storage::{LocalDisk, Store};

    let site = a_site().await;

    // A theme is what a site wrote under `src/` and `public/` — and nothing
    // else: what decides how a site is built is refused on the way in, so a
    // generator brings the project and the workspace brings the content.
    site.wrote("src/index.html", "<h1>Before it was built</h1>")
        .await;

    let kept_in = std::env::temp_dir().join(format!("mavi-built-{}", Uuid::now_v7().simple()));

    let mut state = AppState::new(site.db.clone());
    state.store = std::sync::Arc::new(Store::Disk(LocalDisk::at(&kept_in)));
    state.builder = std::sync::Arc::new(Builder::Here(Generator {
        // Named rather than a shell line, which is what the generator takes:
        // a line would be somebody's file name executed one day.
        program: "/bin/sh".to_owned(),
        arguments: vec![
            "-c".to_owned(),
            "mkdir -p dist && printf '<h1>Built from the theme</h1>' > dist/index.html".to_owned(),
        ],
        output: "dist".to_owned(),
        workspaces: std::env::temp_dir().join(format!("mavi-work-{}", Uuid::now_v7().simple())),
        at_once: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
    }));

    let (status, asked) = site.send("POST", "/api/design/publishes", None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{asked}");

    for _ in 0..4 {
        mavi::jobs::tick(&state, "test").await.expect("tick");
    }

    let router = mavi::router(state);
    let served = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/")
                .header(header::HOST, &site.host)
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");

    assert_eq!(served.status(), StatusCode::OK);

    let bytes = served
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();
    let page = String::from_utf8_lossy(&bytes);

    assert!(
        page.contains("Built from the theme"),
        "what the generator produced is what a visitor gets: {page}"
    );

    std::fs::remove_dir_all(&kept_in).ok();
}
