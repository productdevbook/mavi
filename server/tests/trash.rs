//! What a site threw away, and putting it back.

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
}

async fn a_site() -> Site {
    let db = harness().await;
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
        db,
        router,
        host,
        tenant,
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

    async fn a_post(&self, title: &str) -> Uuid {
        let (status, post) = self
            .send(
                "POST",
                "/api/posts",
                Some(serde_json::json!({ "language": "en", "title": title })),
            )
            .await;

        assert_eq!(status, StatusCode::CREATED, "{post}");

        post["id"].as_str().expect("an id").parse().expect("a uuid")
    }
}

#[tokio::test]
async fn something_thrown_away_is_in_the_trash_and_comes_back() {
    let site = a_site().await;
    let id = site.a_post("A Mistake").await;

    site.send("DELETE", &format!("/api/posts/{id}"), None).await;

    let (status, thrown) = site.send("GET", "/api/trash", None).await;

    assert_eq!(status, StatusCode::OK);

    let mine = thrown["items"]
        .as_array()
        .expect("a page")
        .iter()
        .find(|one| one["id"] == id.to_string())
        .expect("the post that was thrown away");

    assert_eq!(mine["kind"], "posts");
    assert_eq!(mine["name"], "A Mistake");
    assert!(mine["goes_at"].is_string(), "nothing says when it goes");

    let (status, back) = site
        .send("POST", &format!("/api/trash/posts/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::OK, "{back}");

    let (status, post) = site.send("GET", &format!("/api/posts/{id}"), None).await;

    assert_eq!(status, StatusCode::OK, "it did not come back: {post}");
    assert_eq!(post["post"]["title"], "A Mistake");
}

#[tokio::test]
async fn something_that_was_never_thrown_away_cannot_be_put_back() {
    let site = a_site().await;
    let id = site.a_post("Still Here").await;

    let (status, _) = site
        .send("POST", &format!("/api/trash/posts/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_kind_of_thing_that_is_not_thrown_away_is_not_a_kind() {
    let site = a_site().await;

    let (status, _) = site
        .send(
            "POST",
            &format!("/api/trash/users/{}", Uuid::now_v7()),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn putting_something_back_where_its_name_has_been_taken_says_so() {
    let site = a_site().await;
    let id = site.a_post("Twice Over").await;

    site.send("DELETE", &format!("/api/posts/{id}"), None).await;

    // Somebody writes another one under the same address while the first is
    // in the trash.
    site.a_post("Twice Over").await;

    let (status, body) = site
        .send("POST", &format!("/api/trash/posts/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
}

#[tokio::test]
async fn something_can_be_taken_away_for_good() {
    let site = a_site().await;
    let id = site.a_post("Gone For Good").await;

    site.send("DELETE", &format!("/api/posts/{id}"), None).await;

    let (status, _) = site
        .send("DELETE", &format!("/api/trash/posts/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = site
        .send("POST", &format!("/api/trash/posts/{id}"), None)
        .await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "it was still there to put back"
    );
}

#[tokio::test]
async fn the_trash_empties_itself_after_a_while() {
    let site = a_site().await;
    let id = site.a_post("Long Ago").await;

    site.send("DELETE", &format!("/api/posts/{id}"), None).await;

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");
    sqlx::query("update posts set deleted_at = now() - interval '60 days' where id = $1")
        .bind(id)
        .execute(conn.conn())
        .await
        .expect("walk back");
    conn.commit().await.expect("commit");

    let state = AppState::new(site.db.clone());
    let taken = mavi::trash::empty(&state, site.tenant)
        .await
        .expect("empty");

    assert_eq!(taken, 1);

    let (status, thrown) = site.send("GET", "/api/trash", None).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        !thrown["items"]
            .as_array()
            .expect("a page")
            .iter()
            .any(|one| one["id"] == id.to_string()),
        "something past its time is still in the trash"
    );
}
