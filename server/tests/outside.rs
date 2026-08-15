//! Proves the seam something outside this crate builds on: it can hand in its
//! own endpoint and its own kind of work, and both go through exactly what a
//! domain built into this crate goes through — the same [`Guard`], the same
//! audit rule, the same queue.
use std::sync::Arc;

use axum::Json;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use mavi::kernel::outside::{JobFuture, Outside};
use mavi::kernel::queue::{self, Job, Task};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;
use uuid::Uuid;

mod common;
use common::{a_role, a_tenant, a_user, harness};

const PASSWORD: &str = "a long enough password";

/// A kind of work this crate has never heard of.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Beacon;

impl Task for Beacon {
    const KIND: &'static str = "outside.beacon";
}

fn run_beacon<'a>(_state: &'a AppState, _job: &'a Job) -> JobFuture<'a> {
    Box::pin(async { Ok(()) })
}

/// An endpoint this crate has never heard of, reached only by somebody signed
/// in — [`Caller`] is filled in whoever is asking, and `admitting` is what
/// turns "signed in or not" into a refusal before this is ever called.
async fn seen(_caller: Caller) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "seen": true }))
}

fn an_outside_crate() -> Outside {
    Outside {
        endpoints: vec![
            Endpoint::get(
                "/api/outside/seen",
                Guard {
                    audience: Audience::User,
                    needs: None,
                    rate: RatePolicy::None,
                },
                seen,
            )
            .within("outside"),
        ],
        jobs: vec![("outside.beacon", run_beacon)],
    }
}

#[tokio::test]
async fn an_outside_endpoint_answers_and_its_guard_is_enforced() {
    let db = harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());
    let tenant = a_tenant(&db, &host).await;
    let role = a_role(&db, tenant, "owner", &every_grant()).await;
    let (_, email) = a_user(&db, tenant, role, PASSWORD).await;

    let mut state = AppState::new(db);
    state.outside = Arc::new(an_outside_crate());
    let router = mavi::router(state);

    // Nobody signed in: the same `Audience::User` check a built-in endpoint
    // is refused by turns this one away too.
    let anonymous = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/outside/seen")
                .header(header::HOST, &host)
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");

    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let signed_in = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/session")
                .header(header::HOST, &host)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "email": email, "password": PASSWORD }).to_string(),
                ))
                .expect("a request"),
        )
        .await
        .expect("a response");

    assert_eq!(signed_in.status(), StatusCode::OK);

    let bytes = signed_in
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();
    let token = serde_json::from_slice::<serde_json::Value>(&bytes).expect("json")["token"]
        .as_str()
        .expect("a token")
        .to_owned();

    let answered = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/outside/seen")
                .header(header::HOST, &host)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");

    assert_eq!(answered.status(), StatusCode::OK);

    let bytes = answered
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(body["seen"], true);
}

#[tokio::test]
#[should_panic(expected = "content")]
async fn an_outside_endpoint_cannot_claim_a_domain_this_crate_already_answers_under() {
    let db = harness().await;
    let mut outside = Outside::default();
    outside.endpoints.push(
        Endpoint::get(
            "/api/outside/stolen",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::None,
            },
            seen,
        )
        .within("content"),
    );

    let mut state = AppState::new(db);
    state.outside = Arc::new(outside);

    let _ = mavi::router(state);
}

#[tokio::test]
async fn an_outside_endpoint_appears_in_the_description_the_server_serves() {
    let db = harness().await;
    let mut state = AppState::new(db);
    state.outside = Arc::new(an_outside_crate());
    let router = mavi::router(state);

    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/openapi.json")
                .header(header::HOST, "operator.invalid")
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");

    assert_eq!(response.status(), StatusCode::OK);

    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();
    let description: serde_json::Value = serde_json::from_slice(&bytes).expect("json");

    assert!(
        description["paths"]
            .as_object()
            .expect("paths")
            .contains_key("/api/outside/seen"),
        "the outside endpoint is missing from the description the server serves"
    );
}

#[tokio::test]
async fn a_job_of_an_outside_kind_is_claimed_and_run_by_the_core_worker_loop() {
    let db = harness().await;
    let tenant = a_tenant(&db, &format!("{}.example", Uuid::now_v7().simple())).await;

    let mut state = AppState::new(db.clone());
    state.outside = Arc::new(an_outside_crate());

    let mut conn = db.tenant(tenant).await.expect("begin");
    queue::enqueue(&mut conn, &Beacon, None)
        .await
        .expect("queued");
    conn.commit().await.expect("commit");

    let claimed = mavi::jobs::tick_within(&state, "a test", Some(tenant))
        .await
        .expect("tick");

    assert!(claimed, "a job of an outside kind was never claimed");

    let mut conn = db.tenant(tenant).await.expect("begin");
    let left: i64 =
        sqlx::query_scalar("select count(*) from jobs where tenant_id = $1 and state <> 'done'")
            .bind(tenant.0)
            .fetch_one(conn.conn())
            .await
            .expect("count");

    assert_eq!(left, 0, "the outside job was claimed but never finished");
}
