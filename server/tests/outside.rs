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
use mavi::kernel::scheduler::Every;
use serde::{Deserialize, Serialize};
use sqlx::Connection as _;
use tower::ServiceExt;
use uuid::Uuid;

mod common;
use common::harness;
use mavi::testing::{APP_ROLE, a_role, a_user};

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
        migrations: None,
        schedules: vec![("outside.beacon", Every::Day)],
        policies: Vec::new(),
    }
}

#[tokio::test]
async fn an_outside_endpoint_answers_and_its_guard_is_enforced() {
    let db = harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());
    let role = a_role(&db, "owner", &every_grant()).await;
    let (_, email) = a_user(&db, role, PASSWORD).await;

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
                .header(header::HOST, "somewhere.invalid")
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

/// A database nothing else has touched, reached as whoever may make tables in
/// it. The leased ones are handed out as the role a request is served as,
/// which by design cannot: row-level security has no effect on a superuser,
/// so the role every other test runs as is not one.
async fn a_database_of_its_own() -> (mavi::kernel::db::Db, String) {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let name = format!("v1_outside_{}", Uuid::now_v7().simple());

    let mut making = sqlx::PgConnection::connect(&url).await.expect("connect");
    sqlx::query(&format!("create database {name}"))
        .execute(&mut making)
        .await
        .expect("a database of its own");

    let (before, _) = url.rsplit_once('/').expect("a database in the url");
    let its_own = format!("{before}/{name}");

    let admin = mavi::kernel::db::Db::connect(&its_own, 4)
        .await
        .expect("connect");

    (admin, its_own)
}

/// A database made here rather than one of the leased ones: what a test
/// leaves in a leased database is emptied out of it, and a migration is not a
/// row anybody wrote — `900000001` would still be recorded as run when the
/// next test was handed it.
///
/// Migrates and grants exactly as [`start`](mavi::start::start) does: this
/// crate's own migrations, unqualified — the same connection an outside
/// crate's migrations run as — then the outside migration, then the
/// row-scoped role every other test in this file already runs requests as.
async fn an_outside_database() -> mavi::kernel::db::Db {
    let (admin, its_own) = a_database_of_its_own().await;
    admin.migrate().await.expect("this crate's own migrations");

    let migrator = sqlx::migrate::Migrator::new(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/outside_migrations"
    )))
    .await
    .expect("an outside migrator");

    admin
        .migrate_with(migrator)
        .await
        .expect("the outside migration ran");

    let mut tx = admin.begin().await.expect("begin");

    sqlx::query(&format!(
        "do $$ begin
             if not exists (select from pg_roles where rolname = '{APP_ROLE}') then
                 create role {APP_ROLE} nologin;
             end if;
         end $$;"
    ))
    .execute(tx.conn())
    .await
    .expect("role");

    for grant in [
        format!("grant usage on schema public to {APP_ROLE}"),
        format!(
            "grant select, insert, update, delete on all tables in schema public to {APP_ROLE}"
        ),
    ] {
        sqlx::query(&grant).execute(tx.conn()).await.expect("grant");
    }
    tx.commit().await.expect("commit");

    mavi::kernel::db::Db::connect_as(&its_own, 8, Some(APP_ROLE))
        .await
        .expect("connect as app")
}

#[tokio::test]
async fn an_outside_migration_runs_before_the_endpoint_and_job_it_carries_in_are_used() {
    let db = an_outside_database().await;

    let mut state = AppState::new(db.clone());
    state.outside = Arc::new(an_outside_crate());
    let router = mavi::router(state.clone());

    let host = format!("{}.example", Uuid::now_v7().simple());
    let role = a_role(&db, "owner", &every_grant()).await;
    let (_, email) = a_user(&db, role, PASSWORD).await;

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

    // The endpoint answers through its guard.
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

    // The job kind is claimed by the core worker loop.
    let mut conn = db.begin().await.expect("begin");
    queue::enqueue(&mut conn, &Beacon, None)
        .await
        .expect("queued");
    conn.commit().await.expect("commit");

    let claimed = mavi::jobs::tick(&state, "a test").await.expect("tick");
    assert!(claimed, "a job of an outside kind was never claimed");

    // And the table the outside migration created is really there, usable by
    // the role this process runs as day to day, not just by the admin
    // connection that migrated it.
    let mut tx = db.begin().await.expect("begin");
    sqlx::query("insert into outside_beacons default values")
        .execute(tx.conn())
        .await
        .expect("insert into the outside table");
    let count: i64 = sqlx::query_scalar("select count(*) from outside_beacons")
        .fetch_one(tx.conn())
        .await
        .expect("count");
    assert_eq!(count, 1, "the outside migration never ran");
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn a_job_of_an_outside_kind_is_claimed_and_run_by_the_core_worker_loop() {
    let db = harness().await;

    let mut state = AppState::new(db.clone());
    state.outside = Arc::new(an_outside_crate());

    let mut conn = db.begin().await.expect("begin");
    queue::enqueue(&mut conn, &Beacon, None)
        .await
        .expect("queued");
    conn.commit().await.expect("commit");

    let claimed = mavi::jobs::tick(&state, "a test").await.expect("tick");

    assert!(claimed, "a job of an outside kind was never claimed");

    let mut conn = db.begin().await.expect("begin");
    let left: i64 = sqlx::query_scalar("select count(*) from jobs where state <> 'done'")
        .fetch_one(conn.conn())
        .await
        .expect("count");

    assert_eq!(left, 0, "the outside job was claimed but never finished");
}

/// Both crates' migrations are tracked in one table, so a version they have
/// both used is a checksum sqlx will one day disagree with — long after
/// whoever numbered it has forgotten. It is refused at the first run instead,
/// and told which number caused it.
#[tokio::test]
async fn an_outside_migration_numbered_like_one_of_ours_is_refused() {
    let db = harness().await;

    let refused = db
        .migrate_with(sqlx::migrate!("./tests/fixtures/colliding_migrations"))
        .await
        .expect_err("a version this crate has already used");

    assert!(
        refused.to_string().contains("numbered 1"),
        "the refusal has to name the number: {refused}"
    );
}

/// The same seam `jobs::schedule_due` runs this crate's own daily and hourly
/// work through also carries an outside crate's: nothing puts a job in the
/// queue unless something schedules it, whoever it belongs to.
#[tokio::test]
async fn an_outside_schedule_puts_its_job_in_the_queue() {
    let db = harness().await;

    let mut state = AppState::new(db.clone());
    state.outside = Arc::new(an_outside_crate());

    mavi::jobs::schedule_due(&state).await.expect("schedule");

    let mut conn = db.begin().await.expect("begin");
    let queued: i64 = sqlx::query_scalar("select count(*) from jobs where kind = 'outside.beacon'")
        .fetch_one(conn.conn())
        .await
        .expect("count");

    assert_eq!(queued, 1, "the outside schedule never queued its job");
}

/// A schedule naming a kind nothing handed in as a job would sit in the
/// queue and nothing would ever claim it — refused at startup instead, and
/// told which kind.
#[test]
#[should_panic(expected = "outside.ghost")]
fn a_schedule_for_a_job_never_handed_in_is_refused() {
    let outside = Outside {
        schedules: vec![("outside.ghost", Every::Day)],
        ..Outside::default()
    };

    mavi::jobs::assert_schedules_are_runnable(&outside);
}

/// The same gate `server/tests/schema.rs` runs over this crate's own
/// [`retention::POLICIES`], run again over a policy an outside crate hands
/// in: a policy naming a sweep with no job for it is a table nobody empties.
#[test]
fn an_outside_retention_policy_is_held_to_the_same_gate() {
    use mavi::kernel::retention::{self, Keeps, Policy};

    let outside = Outside {
        jobs: vec![("outside.ledger.sweep", run_beacon)],
        policies: vec![Policy {
            table: "outside_ledger",
            keeps: Keeps::Days(3650),
            swept_by: "outside.ledger.sweep",
        }],
        ..Outside::default()
    };

    let kinds = mavi::jobs::kinds(&outside);

    for policy in retention::all(&outside) {
        if matches!(policy.keeps, Keeps::WithItsSubject) {
            continue;
        }

        assert!(
            kinds.contains(&policy.swept_by.to_owned()),
            "{} says {} takes it away, and there is no such job",
            policy.table,
            policy.swept_by
        );
    }
}

/// The one that would have been found by a machine that came up once and
/// never again: everything that has run is recorded in one table, so after an
/// outside crate's migrations are in it, this crate's own run has to go on
/// working — every restart is that run.
///
/// A database of its own, for the same reason the one above has one, and
/// reached as whoever may make a table in it: a migration is DDL, and the role
/// a request is served as deliberately cannot.
#[tokio::test]
async fn this_crate_still_migrates_after_an_outside_crate_has() {
    let (db, _) = a_database_of_its_own().await;

    db.migrate_with(sqlx::migrate!("./tests/fixtures/outside_migrations"))
        .await
        .expect("an outside crate's own");

    db.migrate()
        .await
        .expect("this crate's own, with theirs already recorded");
}
