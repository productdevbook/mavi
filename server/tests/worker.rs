//! The loop that takes work off the queue, and stops when it is told to.
//!
//! On a database of its own, because a scheduler is a thing about a whole
//! machine: run against the shared one it schedules a day's work for every
//! site every other test made, and two of these racing for the scheduler's
//! lock make each other look broken.

use std::time::Duration;

use mavi::kernel::http::AppState;
use mavi::kernel::worker;
use tokio::sync::watch;
use uuid::Uuid;

mod common;

use common::{a_machine_of_its_own, a_tenant};

#[tokio::test]
async fn a_worker_takes_what_is_waiting_and_stops_when_it_is_asked_to() {
    // A machine of its own: what is being tested is a loop that puts work in
    // the queue for every site there is, and every other test's site is not
    // this test's business.
    let db = a_machine_of_its_own().await;
    let tenant = a_tenant(&db, &format!("{}.example", Uuid::now_v7().simple())).await;
    let state = AppState::new(db.clone());

    // Something real and harmless: sweeping a site that has nothing to sweep
    // still has to be claimed, run, and marked done.
    let mut conn = db.tenant(tenant).await.expect("begin");

    mavi::kernel::queue::enqueue(&mut conn, &mavi::housekeeping::SweepSessions, None)
        .await
        .expect("queued");

    conn.commit().await.expect("commit");

    let (say, hear) = watch::channel(false);
    let working = tokio::spawn(worker::work(state, "a test".to_owned(), hear));

    // Waited for rather than slept through: what is being asked is whether the
    // loop does the work at all, and a fixed sleep is how that becomes flaky.
    let done = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let mut conn = db.tenant(tenant).await.expect("begin");

            let (waiting,): (i64,) = sqlx::query_as(
                "select count(*) from jobs where tenant_id = $1 and state <> 'done'",
            )
            .bind(tenant.0)
            .fetch_one(conn.conn())
            .await
            .expect("a count");

            conn.commit().await.expect("commit");

            if waiting == 0 {
                break;
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;

    assert!(done.is_ok(), "a worker did not take what was waiting");

    say.send(true).expect("say stop");

    let stopped = tokio::time::timeout(Duration::from_secs(10), working).await;

    assert!(
        stopped.is_ok(),
        "a worker that was asked to stop was still going"
    );
}

#[tokio::test]
async fn keeping_time_puts_the_day_s_work_in_the_queue() {
    // A machine of its own: what is being tested is a loop that puts work in
    // the queue for every site there is, and every other test's site is not
    // this test's business.
    let db = a_machine_of_its_own().await;
    let tenant = a_tenant(&db, &format!("{}.example", Uuid::now_v7().simple())).await;
    let state = AppState::new(db.clone());

    mavi::jobs::schedule_due(&state).await.expect("scheduled");

    let mut conn = db.tenant(tenant).await.expect("begin");

    let kinds: Vec<(String,)> = sqlx::query_as("select kind from jobs where tenant_id = $1")
        .bind(tenant.0)
        .fetch_all(conn.conn())
        .await
        .expect("kinds");

    conn.commit().await.expect("commit");

    let kinds: Vec<String> = kinds.into_iter().map(|(kind,)| kind).collect();

    for wanted in ["content.publish-due", "sessions.sweep", "domains.check"] {
        assert!(
            kinds.iter().any(|kind| kind == wanted),
            "nothing scheduled {wanted}: {kinds:?}"
        );
    }
}

#[tokio::test]
async fn what_is_scheduled_is_scheduled_once() {
    // A machine of its own: what is being tested is a loop that puts work in
    // the queue for every site there is, and every other test's site is not
    // this test's business.
    let db = a_machine_of_its_own().await;
    let tenant = a_tenant(&db, &format!("{}.example", Uuid::now_v7().simple())).await;
    let state = AppState::new(db.clone());

    for _ in 0..3 {
        mavi::jobs::schedule_due(&state).await.expect("scheduled");
    }

    let mut conn = db.tenant(tenant).await.expect("begin");

    let (sweeps,): (i64,) = sqlx::query_as(
        "select count(*) from jobs where tenant_id = $1 and kind = 'sessions.sweep'",
    )
    .bind(tenant.0)
    .fetch_one(conn.conn())
    .await
    .expect("a count");

    conn.commit().await.expect("commit");

    assert_eq!(
        sweeps, 1,
        "a scheduler that looks every minute left a day's work in the queue three times"
    );
}
