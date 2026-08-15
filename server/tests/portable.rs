//! Taking a site's content out, and reading it into another one.
//!
//! Which is another machine: there is one site to an installation, so the
//! far end of a round trip is a second one of those rather than a second row.

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
use mavi::testing::{a_role, a_tenant, a_user};

struct Site {
    router: axum::Router,
    host: String,
    token: String,
}

async fn a_site() -> Site {
    on(harness().await).await
}

/// The far end of a round trip: a machine that has never seen any of this.
async fn a_site_somewhere_else() -> Site {
    on(mavi::testing::another_machine().await).await
}

async fn on(db: Db) -> Site {
    let host = format!("{}.example", Uuid::now_v7().simple());
    let tenant = a_tenant(&db, &host).await;
    let role = a_role(&db, tenant, "owner", &every_grant()).await;
    let password = "a long enough password";
    let (_, email) = a_user(&db, tenant, role, password).await;

    let mut conn = db.tenant(tenant).await.expect("begin");
    sqlx::query(
        "insert into languages (tenant_id, code, name, is_default)
         values ($1, 'en', 'English', true)",
    )
    .bind(tenant.0)
    .execute(conn.conn())
    .await
    .expect("a language");
    conn.commit().await.expect("commit");

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

async fn a_site_with_something_on_it() -> Site {
    let site = a_site().await;

    let (_, term) = site
        .send(
            "POST",
            "/api/terms",
            Some(serde_json::json!({
                "kind": "category", "language": "en", "name": "Recipes"
            })),
        )
        .await;

    let (_, post) = site
        .send(
            "POST",
            "/api/posts",
            Some(serde_json::json!({
                "language": "en", "title": "A Quick One", "body": "Something",
                "fields": {}
            })),
        )
        .await;

    site.send(
        "PUT",
        &format!("/api/posts/{}/terms", post["id"].as_str().expect("an id")),
        Some(serde_json::json!({ "term_ids": [term["id"]] })),
    )
    .await;

    site
}

#[tokio::test]
async fn a_site_goes_out_and_comes_back_into_another_one() {
    let one = a_site_with_something_on_it().await;
    let two = a_site_somewhere_else().await;

    let (status, bundle) = one.send("GET", "/api/portable/export", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(bundle["version"], 1);
    assert_eq!(bundle["posts"].as_array().expect("posts").len(), 1);
    assert_eq!(bundle["terms"].as_array().expect("terms").len(), 1);

    let (status, read) = two
        .send("POST", "/api/portable/import", Some(bundle.clone()))
        .await;

    assert_eq!(status, StatusCode::CREATED, "{read}");
    assert_eq!(read["posts"], 1);
    assert_eq!(read["terms"], 1);

    let (_, posts) = two.send("GET", "/api/posts", None).await;
    let post = &posts["items"][0];

    assert_eq!(post["title"], "A Quick One");
    assert!(post["fields"].is_object());
    assert_eq!(
        post["state"], "draft",
        "reading a bundle in published somebody's pages for them"
    );

    // And what it was filed under came with it.
    let (_, terms) = two.send("GET", "/api/terms", None).await;
    let term = &terms["items"][0];

    let (_, filed) = two
        .send(
            "GET",
            &format!("/api/posts?term={}", term["id"].as_str().expect("an id")),
            None,
        )
        .await;

    assert_eq!(
        filed["items"].as_array().expect("a list").len(),
        1,
        "the post lost what it was filed under on the way"
    );
}

#[tokio::test]
async fn reading_the_same_bundle_twice_is_one_site() {
    let one = a_site_with_something_on_it().await;
    let two = a_site_somewhere_else().await;

    let (_, bundle) = one.send("GET", "/api/portable/export", None).await;

    let (_, first) = two
        .send("POST", "/api/portable/import", Some(bundle.clone()))
        .await;
    let (_, again) = two.send("POST", "/api/portable/import", Some(bundle)).await;

    assert_eq!(first["posts"], 1);
    assert_eq!(again["posts"], 0, "the second read made another copy");
    assert_eq!(
        again["left_alone"], 2,
        "what was already there was not counted"
    );

    let (_, posts) = two.send("GET", "/api/posts", None).await;

    assert_eq!(posts["items"].as_array().expect("a list").len(), 1);
}

#[tokio::test]
async fn a_bundle_from_a_version_this_does_not_know_is_refused() {
    let site = a_site().await;

    let (status, _) = site
        .send(
            "POST",
            "/api/portable/import",
            Some(serde_json::json!({ "version": 99, "posts": [] })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_bundle_written_in_a_language_the_site_does_not_have_says_so() {
    let one = a_site_with_something_on_it().await;
    let two = a_site_somewhere_else().await;

    let (_, mut bundle) = one.send("GET", "/api/portable/export", None).await;

    // The language is taken out of the bundle, and the posts still claim it.
    bundle["languages"] = serde_json::json!([]);

    let (status, body) = two.send("POST", "/api/portable/import", Some(bundle)).await;

    assert_eq!(status, StatusCode::CREATED, "{body}");

    // 'en' happens to exist on every site made by this test, so the more
    // interesting case is a language nothing has.
    let (_, mut bundle) = one.send("GET", "/api/portable/export", None).await;
    bundle["languages"] = serde_json::json!([]);
    bundle["posts"][0]["language"] = serde_json::json!("fr");
    bundle["posts"][0]["slug"] = serde_json::json!("une-recette");

    let (status, _) = two.send("POST", "/api/portable/import", Some(bundle)).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_bundle_with_something_in_it_this_does_not_know_is_refused() {
    let site = a_site().await;

    let (status, _) = site
        .send(
            "POST",
            "/api/portable/import",
            Some(serde_json::json!({
                "version": 1,
                "posts": [],
                "something_else": ["from a later version"]
            })),
        )
        .await;

    assert!(
        status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::BAD_REQUEST,
        "a bundle carrying something unknown was read anyway"
    );
}
