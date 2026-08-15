//! The first run of a machine, and the fact that there is only one.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::harness;

struct Machine {
    db: Db,
    router: axum::Router,
}

impl Machine {
    async fn new() -> Self {
        let db = harness().await;

        Self {
            router: mavi::router(AppState::new(db.clone())),
            db,
        }
    }

    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        // The machine's own screens are reached on an address that is nobody's
        // site, which is what makes this the console rather than a site.
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "console.example");

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

    fn somebody() -> serde_json::Value {
        serde_json::json!({
            "email": format!("runs-it-{}@example.test", Uuid::now_v7().simple()),
            "name": "Somebody",
            "password": "a long enough password",
        })
    }
}

#[tokio::test]
async fn a_machine_with_nobody_on_it_says_it_is_waiting() {
    let machine = Machine::new().await;

    let (status, waiting) = machine.send("GET", "/api/setup", None).await;

    assert_eq!(status, StatusCode::OK, "{waiting}");
    assert_eq!(waiting["needed"], true);
}

#[tokio::test]
async fn the_first_account_is_made_once_and_the_door_shuts() {
    let machine = Machine::new().await;
    let who = Machine::somebody();

    let (status, made) = machine.send("POST", "/api/setup", Some(who.clone())).await;

    assert_eq!(status, StatusCode::CREATED, "{made}");
    assert_eq!(made["needed"], false);

    let (status, waiting) = machine.send("GET", "/api/setup", None).await;

    assert_eq!(waiting["needed"], false, "{status}");
}

#[tokio::test]
async fn a_second_one_is_refused_however_it_arrives() {
    let machine = Machine::new().await;

    machine
        .send("POST", "/api/setup", Some(Machine::somebody()))
        .await;

    let (status, refused) = machine
        .send("POST", "/api/setup", Some(Machine::somebody()))
        .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a second account was made through the door that is only open once: {refused}"
    );
    assert_eq!(refused["error"]["key"], "this_machine_is_already_set_up");
}

#[tokio::test]
async fn two_arriving_together_make_one_account() {
    let machine = Machine::new().await;

    let (one, two) = tokio::join!(
        machine.send("POST", "/api/setup", Some(Machine::somebody())),
        machine.send("POST", "/api/setup", Some(Machine::somebody())),
    );

    let made = [one.0, two.0]
        .iter()
        .filter(|status| **status == StatusCode::CREATED)
        .count();

    assert_eq!(made, 1, "{one:?} and {two:?}");

    let mut conn = machine.db.operator().await.expect("begin");

    let (accounts,): (i64,) = sqlx::query_as("select count(*) from operators")
        .fetch_one(conn.conn())
        .await
        .expect("a count");

    conn.commit().await.expect("commit");

    assert_eq!(
        accounts, 1,
        "two requests made two people who run the machine"
    );
}

#[tokio::test]
async fn setting_up_gives_a_site_you_can_sign_into_and_write_in() {
    let machine = Machine::new().await;
    let who = Machine::somebody();

    let (status, made) = machine.send("POST", "/api/setup", Some(who.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{made}");

    let mut lookup = machine.db.operator().await.expect("begin");
    lookup.across_sites().await.expect("across sites");

    let (tenant_id,): (Uuid,) =
        sqlx::query_as("select tenant_id from tenant_domains where host = 'console.example'")
            .fetch_one(lookup.conn())
            .await
            .expect("a site made by setup");

    lookup.commit().await.expect("commit");

    let mut conn = machine
        .db
        .tenant(mavi::kernel::tenant::TenantId(tenant_id))
        .await
        .expect("begin");
    sqlx::query(
        "insert into languages (tenant_id, code, name, is_default) values ($1, 'en', 'English', true)",
    )
    .bind(tenant_id)
    .execute(conn.conn())
    .await
    .expect("a language");
    conn.commit().await.expect("commit");

    let (status, signed_in) = machine
        .send(
            "POST",
            "/api/auth/session",
            Some(serde_json::json!({
                "email": who["email"],
                "password": who["password"],
            })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{signed_in}");
    let token = signed_in["token"].as_str().expect("a token").to_owned();

    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/api/posts")
        .header(header::HOST, "console.example")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({ "language": "en", "title": "The first post" }).to_string(),
        ))
        .expect("a request");

    let response = machine
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
    let posted: serde_json::Value =
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);

    assert_eq!(status, StatusCode::CREATED, "{posted}");
}

#[tokio::test]
async fn a_password_nobody_could_call_one_is_refused() {
    let machine = Machine::new().await;
    let mut who = Machine::somebody();
    who["password"] = serde_json::Value::String("short".to_owned());

    let (status, refused) = machine.send("POST", "/api/setup", Some(who)).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}
