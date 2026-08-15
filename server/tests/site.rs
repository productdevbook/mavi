//! What a site will tell somebody about themselves, and what it will take
//! away when they ask.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use mavi::kernel::tenant::TenantId;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::harness;
use mavi::testing::{a_role, a_tenant, a_user};

struct Site {
    db: Db,
    router: axum::Router,
    host: String,
    tenant: TenantId,
    token: String,
    email: String,
    owner_role: Uuid,
}

async fn a_site() -> Site {
    a_site_where_somebody_can(&every_grant()).await
}

/// A site whose account holds exactly these grants — for asking whether
/// reaching something is gated on one of them.
async fn a_site_where_somebody_can(grants: &[String]) -> Site {
    let db = harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());
    let tenant = a_tenant(&db, &host).await;
    let role = a_role(&db, tenant, "somebody", grants).await;
    let password = "a long enough password";
    let (_, email) = a_user(&db, tenant, role, password).await;

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
        tenant,
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

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    sqlx::query(
        "insert into subscribers (tenant_id, email, token_hash)
         values ($1, $2, sha256($3::bytea))",
    )
    .bind(site.tenant.0)
    .bind(&address)
    .bind(Uuid::now_v7().as_bytes().to_vec())
    .execute(conn.conn())
    .await
    .expect("a subscriber");

    sqlx::query("insert into email_log (tenant_id, to_email, subject) values ($1, $2, 'Hello')")
        .bind(site.tenant.0)
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

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    sqlx::query(
        "insert into subscribers (tenant_id, email, token_hash)
         values ($1, $2, sha256($3::bytea))",
    )
    .bind(site.tenant.0)
    .bind(&address)
    .bind(Uuid::now_v7().as_bytes().to_vec())
    .execute(conn.conn())
    .await
    .expect("a subscriber");

    sqlx::query(
        "insert into orders (tenant_id, email, total_minor, currency, idempotency_key)
         values ($1, $2, 1000, 'TRY', $3)",
    )
    .bind(site.tenant.0)
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

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

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

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

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

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");
    let hash = mavi::kernel::password::hash("a long enough password").expect("hash");
    let second = format!("second-owner-{}@example.test", Uuid::now_v7().simple());

    sqlx::query(
        "insert into users (tenant_id, role_id, email, name, password_hash, state)
         values ($1, $2, $3, 'Another Owner', $4, 'active')",
    )
    .bind(site.tenant.0)
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

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

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

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    sqlx::query(
        "insert into languages (tenant_id, code, name, is_default)
         values ($1, 'en', 'English', true)",
    )
    .bind(site.tenant.0)
    .execute(conn.conn())
    .await
    .expect("a language");

    sqlx::query(
        "insert into posts (tenant_id, language, slug, title, state, published_at, excerpt)
         values ($1, 'en', 'hello', 'Hello There', 'published', now(), 'A first post')",
    )
    .bind(site.tenant.0)
    .execute(conn.conn())
    .await
    .expect("a post");

    sqlx::query(
        "insert into posts (tenant_id, language, slug, title, state)
         values ($1, 'en', 'later', 'Not Yet', 'draft')",
    )
    .bind(site.tenant.0)
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

/// What `/api/site/usage` answers, fetched once and read from by whichever
/// test is asking about one part of it.
async fn usage_of(site: &Site) -> serde_json::Value {
    let (status, body) = site
        .send("GET", "/api/site/usage", Some(&site.token), None)
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");

    serde_json::from_str(&body).expect("json")
}

#[tokio::test]
async fn storage_and_rows_by_kind_are_read_back_as_what_was_written() {
    let site = a_site().await;
    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    sqlx::query(
        "insert into languages (tenant_id, code, name, is_default)
         values ($1, 'en', 'English', true)",
    )
    .bind(site.tenant.0)
    .execute(conn.conn())
    .await
    .expect("a language");

    for slug in ["first-post", "second-post", "third-post"] {
        sqlx::query(
            "insert into posts (tenant_id, language, slug, title, state)
             values ($1, 'en', $2, $2, 'draft')",
        )
        .bind(site.tenant.0)
        .bind(slug)
        .execute(conn.conn())
        .await
        .expect("a post");
    }

    sqlx::query(
        "insert into media (tenant_id, location, original_name, mime, bytes, checksum)
         values ($1, 'a/one.png', 'one.png', 'image/png', 1000, '\\x01'::bytea),
                ($1, 'a/two.mp4', 'two.mp4', 'video/mp4', 4000, '\\x02'::bytea)",
    )
    .bind(site.tenant.0)
    .execute(conn.conn())
    .await
    .expect("media");

    // `reltuples` is only as fresh as the last analyze; a test asserting an
    // estimate has to ask for one rather than trust it arrived on its own.
    sqlx::query("analyze posts")
        .execute(conn.conn())
        .await
        .expect("analyze");
    sqlx::query("analyze media")
        .execute(conn.conn())
        .await
        .expect("analyze");

    conn.commit().await.expect("commit");

    let read = usage_of(&site).await;

    assert_eq!(read["storage"]["used_bytes"], 5000, "{read}");

    let by_kind = read["storage"]["by_kind"].as_array().expect("kinds");
    let image = by_kind
        .iter()
        .find(|kind| kind["kind"] == "image")
        .expect("an image kind");

    assert_eq!(image["bytes"], 1000, "{read}");
    assert_eq!(image["count"], 1, "{read}");

    let rows = read["rows"].as_array().expect("rows");
    let posts = rows
        .iter()
        .find(|row| row["kind"] == "posts")
        .expect("a posts row");

    assert_eq!(posts["approx_rows"], 3, "{read}");
}

#[tokio::test]
async fn mail_attempted_delivered_bounced_and_failed_are_read_back() {
    let site = a_site().await;
    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    // Not attempted: still queued, nothing has tried it yet.
    sqlx::query(
        "insert into email_log (tenant_id, to_email, subject, state)
         values ($1, 'queued-reader@example.test', 'Hi', 'queued')",
    )
    .bind(site.tenant.0)
    .execute(conn.conn())
    .await
    .expect("a queued message");

    let (sent_id,): (Uuid,) = sqlx::query_as(
        "insert into email_log (tenant_id, to_email, subject, state)
         values ($1, 'sent-reader@example.test', 'Hi', 'sent') returning id",
    )
    .bind(site.tenant.0)
    .fetch_one(conn.conn())
    .await
    .expect("a sent message");

    sqlx::query(
        "insert into email_log (tenant_id, to_email, subject, state)
         values ($1, 'bounced-reader@example.test', 'Hi', 'bounced')",
    )
    .bind(site.tenant.0)
    .execute(conn.conn())
    .await
    .expect("a bounced message");

    sqlx::query(
        "insert into email_log (tenant_id, to_email, subject, state)
         values ($1, 'failed-reader@example.test', 'Hi', 'failed')",
    )
    .bind(site.tenant.0)
    .execute(conn.conn())
    .await
    .expect("a failed message");

    sqlx::query(
        "insert into mail_events (tenant_id, email_log_id, kind, provider_ref)
         values ($1, $2, 'delivered', $3)",
    )
    .bind(site.tenant.0)
    .bind(sent_id)
    .bind(Uuid::now_v7().to_string())
    .execute(conn.conn())
    .await
    .expect("a delivery");

    conn.commit().await.expect("commit");

    let read = usage_of(&site).await;

    assert_eq!(read["mail"]["attempted"], 3, "{read}");
    assert_eq!(read["mail"]["delivered"], 1, "{read}");
    assert_eq!(read["mail"]["bounced"], 1, "{read}");
    assert_eq!(read["mail"]["failed"], 1, "{read}");
}

#[tokio::test]
async fn builds_and_the_queue_are_read_back() {
    let site = a_site().await;
    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    sqlx::query(
        "insert into publishes (tenant_id, branch, state, seconds, finished_at)
         values ($1, 'live', 'live', 42, now())",
    )
    .bind(site.tenant.0)
    .execute(conn.conn())
    .await
    .expect("a build");

    sqlx::query(
        "insert into jobs (tenant_id, kind, state, run_at)
         values ($1, 'domains.check', 'ready', now() - interval '1 hour')",
    )
    .bind(site.tenant.0)
    .execute(conn.conn())
    .await
    .expect("a waiting job");

    sqlx::query(
        "insert into jobs (tenant_id, kind, state, run_at, claimed_until, claimed_by)
         values ($1, 'domains.check', 'running', now(), now() + interval '5 minutes', 'a-worker')",
    )
    .bind(site.tenant.0)
    .execute(conn.conn())
    .await
    .expect("a running job");

    conn.commit().await.expect("commit");

    let read = usage_of(&site).await;

    assert_eq!(read["builds"][0]["state"], "live", "{read}");
    assert_eq!(read["builds"][0]["seconds"], 42, "{read}");

    assert_eq!(read["queue"]["waiting"], 1, "{read}");
    assert_eq!(read["queue"]["running"], 1, "{read}");
    assert!(
        read["queue"]["oldest_waiting_since"].is_string(),
        "the wait had no age: {read}"
    );
}

#[tokio::test]
async fn reading_what_the_site_is_holding_asks_for_the_grant_that_reads_settings() {
    let site = a_site_where_somebody_can(&["content:view:own".to_owned()]).await;

    let (status, refused) = site
        .send("GET", "/api/site/usage", Some(&site.token), None)
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
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
