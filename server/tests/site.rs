//! What a site will tell somebody about themselves, and what it will take
//! away when they ask.

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
    email: String,
    owner_role: Uuid,
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
        email,
        owner_role: role,
    }
}

impl Site {
    async fn send(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, String) {
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

        (status, String::from_utf8_lossy(&bytes).into_owned())
    }
}

#[tokio::test]
async fn somebody_can_be_given_everything_the_site_holds_about_them() {
    let site = a_site().await;
    let address = format!("reader-{}@example.test", Uuid::now_v7().simple());

    let mut conn = site.db.begin().await.expect("begin");

    sqlx::query(
        "insert into subscribers (email, token_hash)
         values ($1, sha256($2::bytea))",
    )
    .bind(&address)
    .bind(Uuid::now_v7().as_bytes().to_vec())
    .execute(conn.conn())
    .await
    .expect("a subscriber");

    sqlx::query("insert into email_log (to_email, subject) values ($1, 'Hello')")
        .bind(&address)
        .execute(conn.conn())
        .await
        .expect("something sent");

    conn.commit().await.expect("commit");

    let (status, body) = site
        .send(
            "POST",
            "/api/people/export",
            Some(&site.token),
            Some(serde_json::json!({ "email": address })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");

    let copy: serde_json::Value = serde_json::from_str(&body).expect("json");

    assert_eq!(
        copy["found"]["subscribers"].as_array().expect("rows").len(),
        1
    );
    assert_eq!(
        copy["found"]["email_log"].as_array().expect("rows").len(),
        1
    );
    assert!(
        !body.contains("token_hash"),
        "a copy handed somebody a token"
    );
}

#[tokio::test]
async fn erasing_takes_them_away_and_keeps_the_bill() {
    let site = a_site().await;
    let address = format!("buyer-{}@example.test", Uuid::now_v7().simple());

    let mut conn = site.db.begin().await.expect("begin");

    sqlx::query(
        "insert into subscribers (email, token_hash)
         values ($1, sha256($2::bytea))",
    )
    .bind(&address)
    .bind(Uuid::now_v7().as_bytes().to_vec())
    .execute(conn.conn())
    .await
    .expect("a subscriber");

    sqlx::query(
        "insert into orders (email, total_minor, currency, idempotency_key)
         values ($1, 1000, 'TRY', $2)",
    )
    .bind(&address)
    .bind(Uuid::now_v7().to_string())
    .execute(conn.conn())
    .await
    .expect("an order");

    conn.commit().await.expect("commit");

    let (status, body) = site
        .send(
            "POST",
            "/api/people/erase",
            Some(&site.token),
            Some(serde_json::json!({ "email": address })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");

    let mut conn = site.db.begin().await.expect("begin");

    let left: (i64,) = sqlx::query_as("select count(*) from subscribers where email = $1")
        .bind(&address)
        .fetch_one(conn.conn())
        .await
        .expect("a count");

    assert_eq!(left.0, 0);

    let orders: (i64, i64) =
        sqlx::query_as("select count(*), count(*) filter (where email = $1) from orders")
            .bind(&address)
            .fetch_one(conn.conn())
            .await
            .expect("a count");

    assert_eq!(orders.0, 1, "a bill disappeared with the person");
    assert_eq!(orders.1, 0, "the bill still says who they were");
}

#[tokio::test]
async fn erasing_the_site_s_only_owner_is_refused() {
    let site = a_site().await;

    let (status, body) = site
        .send(
            "POST",
            "/api/people/erase",
            Some(&site.token),
            Some(serde_json::json!({ "email": site.email })),
        )
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    let mut conn = site.db.begin().await.expect("begin");

    let left: (i64,) =
        sqlx::query_as("select count(*) from users where email = $1 and deleted_at is null")
            .bind(&site.email)
            .fetch_one(conn.conn())
            .await
            .expect("a count");

    assert_eq!(left.0, 1, "the only owner was taken away anyway");
}

#[tokio::test]
async fn erasing_an_owner_who_is_not_the_last_one_still_works() {
    let site = a_site().await;

    let mut conn = site.db.begin().await.expect("begin");
    let hash = mavi::kernel::password::hash("a long enough password").expect("hash");
    let second = format!("second-owner-{}@example.test", Uuid::now_v7().simple());

    sqlx::query(
        "insert into users (role_id, email, name, password_hash, state)
         values ($1, $2, 'Another Owner', $3, 'active')",
    )
    .bind(site.owner_role)
    .bind(&second)
    .bind(&hash)
    .execute(conn.conn())
    .await
    .expect("a second owner");

    conn.commit().await.expect("commit");

    let (status, body) = site
        .send(
            "POST",
            "/api/people/erase",
            Some(&site.token),
            Some(serde_json::json!({ "email": second })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");

    let mut conn = site.db.begin().await.expect("begin");

    let left: (i64,) = sqlx::query_as("select count(*) from users where email = $1")
        .bind(&second)
        .fetch_one(conn.conn())
        .await
        .expect("a count");

    assert_eq!(left.0, 0, "an owner who was not the last one stayed");
}

#[tokio::test]
async fn a_site_says_what_it_has_published() {
    let site = a_site().await;

    let mut conn = site.db.begin().await.expect("begin");

    sqlx::query(
        "insert into languages (code, name, is_default)
         values ('en', 'English', true)",
    )
    .execute(conn.conn())
    .await
    .expect("a language");

    sqlx::query(
        "insert into posts (language, slug, title, state, published_at, excerpt)
         values ('en', 'hello', 'Hello There', 'published', now(), 'A first post')",
    )
    .execute(conn.conn())
    .await
    .expect("a post");

    sqlx::query(
        "insert into posts (language, slug, title, state)
         values ('en', 'later', 'Not Yet', 'draft')",
    )
    .execute(conn.conn())
    .await
    .expect("a draft");

    conn.commit().await.expect("commit");

    let (status, body) = site.send("GET", "/llms.txt", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("[Hello There](/hello)"), "{body}");
    assert!(body.contains("A first post"));
    assert!(!body.contains("Not Yet"), "a draft was listed as published");
}

#[tokio::test]
async fn a_site_can_be_named_by_its_own_people() {
    let site = a_site().await;

    let (status, named) = site
        .send(
            "PATCH",
            "/api/site",
            Some(&site.token),
            Some(serde_json::json!({ "name": "A Shop Of Some Kind" })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{named}");

    let (status, read) = site.send("GET", "/api/site", Some(&site.token), None).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        read.contains("A Shop Of Some Kind"),
        "the name did not come back: {read}"
    );
}

#[tokio::test]
async fn what_a_site_may_take_is_read_and_not_set() {
    let site = a_site().await;

    let (status, refused) = site
        .send(
            "PATCH",
            "/api/site",
            Some(&site.token),
            Some(serde_json::json!({ "name": "A Site", "storage_limit_bytes": 1 })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a site set its own limit: {refused}"
    );
}

#[tokio::test]
async fn the_operator_s_notes_are_not_the_site_s_to_read() {
    let site = a_site().await;

    site.send(
        "PATCH",
        "/api/site",
        Some(&site.token),
        Some(serde_json::json!({ "name": "A Site" })),
    )
    .await;

    let (_, read) = site.send("GET", "/api/site", Some(&site.token), None).await;

    assert!(
        !read.contains("contact") && !read.contains("notes"),
        "the operator's own record of who to talk to reached the site: {read}"
    );
}
