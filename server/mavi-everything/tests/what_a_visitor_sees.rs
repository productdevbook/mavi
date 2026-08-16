//! A visitor, on the same address as the panel.
//!
//! Everything the edge decides is tested in `mavi-edge` without a database.
//! This is the other half: that a set of changes somebody built and published
//! is what a browser gets, and that nothing published is not.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use mavi_core::grant::Grants;
use mavi_db::Db;
use mavi_everything::mounted::everything;
use mavi_files::InADirectory;
use mavi_http::Caller;
use sqlx::{Connection, PgConnection};
use tower::ServiceExt;
use uuid::Uuid;

fn postgres() -> Option<String> {
    let address = std::env::var("TEST_DATABASE_URL").ok();

    assert!(
        address.is_some() || std::env::var("CI").is_err(),
        "CI has no TEST_DATABASE_URL, so no visitor ever saw anything"
    );

    address
}

async fn fresh(named: &str) -> Db {
    let address = postgres().expect("checked by the caller");
    let named = format!(
        "mavi_visitor_{}_{}",
        named.replace('-', "_"),
        Uuid::now_v7().simple()
    );

    let mut admin = PgConnection::connect(&address).await.expect("a connection");
    sqlx::query(&format!("create database {named}"))
        .execute(&mut admin)
        .await
        .expect("a database of its own");

    let (front, _) = address
        .rsplit_once('/')
        .expect("an address with a database");
    let db = Db::open(&format!("{front}/{named}"), 4)
        .await
        .expect("the new database");

    db.migrate().await.expect("every migration");

    db
}

fn somewhere_for_files() -> Arc<dyn mavi_core::ports::Files> {
    Arc::new(InADirectory::at(
        std::env::temp_dir().join(format!("mavi-{}", Uuid::now_v7())),
    ))
}

/// A visitor is nobody. That is the point of these tests: none of this is
/// reached by holding a token.
fn a_visitor() -> mavi_serve::WhoIsAsking {
    Arc::new(|_| Box::pin(async { Caller::Nobody }))
}

fn an_editor() -> mavi_serve::WhoIsAsking {
    Arc::new(|headers| {
        Box::pin(async move {
            if headers.contains_key("authorization") {
                Caller::AnAccount {
                    id: "01930000-0000-7000-8000-000000000001".to_owned(),
                    grants: Grants::of(["content:view", "content:write"].map(ToOwned::to_owned)),
                    session: None,
                }
            } else {
                Caller::Nobody
            }
        })
    })
}

/// One request as a visitor, and what came back.
async fn visited(
    db: &Db,
    files: &Arc<dyn mavi_core::ports::Files>,
    path: &str,
) -> (StatusCode, Option<String>, Vec<u8>) {
    let answer = everything(db, files, a_visitor())
        .oneshot(
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("an answer");

    let status = answer.status();
    let went = answer
        .headers()
        .get(header::LOCATION)
        .and_then(|to| to.to_str().ok())
        .map(ToOwned::to_owned);
    let body = axum::body::to_bytes(answer.into_body(), 256 * 1024)
        .await
        .expect("a body")
        .to_vec();

    (status, went, body)
}

/// A design with these files in it, built, and live.
async fn a_published_site(
    db: &Db,
    files: &Arc<dyn mavi_core::ports::Files>,
    wrote: &[(&str, &str)],
) {
    let mut tx = db.begin().await.expect("a transaction");
    let change = mavi_design::store::start(&mut tx, "a look")
        .await
        .expect("a set of changes");

    for (path, contents) in wrote {
        mavi_design::store::write_file(&mut tx, change.id, path, contents)
            .await
            .expect("a file");
    }

    tx.commit().await.expect("the writing");

    mavi_everything::building::build(db, files.as_ref(), change.id)
        .await
        .expect("a build");

    let mut tx = db.begin().await.expect("a transaction");
    mavi_design::store::publish(&mut tx, change.id)
        .await
        .expect("it goes live");
    tx.commit().await.expect("it stays live");
}

#[tokio::test]
async fn a_published_page_is_what_a_visitor_gets() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("page").await;
    let files = somewhere_for_files();

    a_published_site(
        &db,
        &files,
        &[
            ("public/index.html", "<h1>The front page</h1>"),
            ("public/about/index.html", "<h1>About</h1>"),
            ("public/styles/site.css", "body { color: teal }"),
            // Not served: `src/` is what a generator reads, and serving it is
            // publishing the thing that makes the pages.
            ("src/pages/index.astro", "---\n---\n<h1>not this</h1>"),
        ],
    )
    .await;

    let (status, _, body) = visited(&db, &files, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8_lossy(&body), "<h1>The front page</h1>");

    // A folder is its index, with or without the slash.
    for path in ["/about", "/about/"] {
        let (status, _, body) = visited(&db, &files, path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_eq!(String::from_utf8_lossy(&body), "<h1>About</h1>", "{path}");
    }

    let (status, _, body) = visited(&db, &files, "/styles/site.css").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8_lossy(&body), "body { color: teal }");

    let (status, _, _) = visited(&db, &files, "/src/pages/index.astro").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "the project a site is built from was served as the site"
    );
}

#[tokio::test]
async fn nothing_published_is_not_a_site_yet() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("empty").await;
    let files = somewhere_for_files();

    let (status, _, _) = visited(&db, &files, "/").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn what_the_api_did_not_describe_is_still_the_api() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("under_api").await;
    let files = somewhere_for_files();

    // A published site, so there is a 404 page to be wrongly served.
    a_published_site(&db, &files, &[("public/404.html", "<h1>Not here</h1>")]).await;

    let (status, _, body) = visited(&db, &files, "/api/nothing-like-this").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body)
            .expect("a refusal shaped like every other")["key"],
        "nothing_answers_there",
        "a client that mistyped a path was handed a page of HTML"
    );
}

#[tokio::test]
async fn a_site_that_has_a_page_for_nothing_being_there_gets_to_use_it() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("not_here").await;
    let files = somewhere_for_files();

    a_published_site(
        &db,
        &files,
        &[
            ("public/index.html", "<h1>The front page</h1>"),
            ("public/404.html", "<h1>Not here</h1>"),
        ],
    )
    .await;

    let (status, _, body) = visited(&db, &files, "/nothing-like-this").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(String::from_utf8_lossy(&body), "<h1>Not here</h1>");

    // A missing stylesheet is not a missing page: answering one with HTML is
    // how a stylesheet becomes a parse error in somebody's console.
    let (status, _, body) = visited(&db, &files, "/styles/gone.css").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_ne!(String::from_utf8_lossy(&body), "<h1>Not here</h1>");
}

#[tokio::test]
async fn renaming_a_page_leaves_its_old_address_working() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("renamed").await;
    let files = somewhere_for_files();

    a_published_site(&db, &files, &[("public/index.html", "<h1>Hello</h1>")]).await;

    let router = everything(&db, &files, an_editor());

    let made = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/writings")
                .header("content-type", "application/json")
                .header("authorization", "Bearer whatever")
                .body(Body::from(
                    serde_json::json!({
                        "kind": "post",
                        "language": "en",
                        "slug": "the-old-name",
                        "title": "A Title",
                        "body": "Something written.",
                    })
                    .to_string(),
                ))
                .expect("a request"),
        )
        .await
        .expect("an answer");

    assert_eq!(made.status(), StatusCode::CREATED);

    let made: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(made.into_body(), 256 * 1024)
            .await
            .expect("a body"),
    )
    .expect("what was written");

    let renamed = router
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!(
                    "/api/writings/{}",
                    made["id"].as_str().expect("an id")
                ))
                .header("content-type", "application/json")
                .header("authorization", "Bearer whatever")
                .body(Body::from(
                    serde_json::json!({ "slug": "the-new-name" }).to_string(),
                ))
                .expect("a request"),
        )
        .await
        .expect("an answer");

    assert_eq!(renamed.status(), StatusCode::OK);

    // The whole point, and what the crate this replaces wrote down and never
    // read: every link anybody had made answered "not here" while the answer
    // sat in a table.
    let (status, went, _) = visited(&db, &files, "/blog/the-old-name").await;

    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(went.as_deref(), Some("/blog/the-new-name"));
}
