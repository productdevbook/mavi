//! The queue, against a real Postgres.
//!
//! Every claim in the module's own documentation is a claim about what two
//! processes do at once, and not one of them can be shown by reading the code.
//! Two workers taking one row, a lease lapsing under somebody, a slow worker
//! finishing work that is no longer theirs: all three are here, and all three
//! are the reason this crate is not a `Vec` behind a mutex.

use mavi_db::Db;
use mavi_work::{Ended, Kind, Nothing, Queue};
use sqlx::{Connection, PgConnection, Row};
use uuid::Uuid;

const SEND: Kind = Kind::new("mail.send", 3);
const SWEEP: Kind = Kind::new("forms.sweep", 1);

fn postgres() -> Option<String> {
    let address = std::env::var("TEST_DATABASE_URL").ok();

    assert!(
        address.is_some() || std::env::var("CI").is_err(),
        "CI has no TEST_DATABASE_URL, so the queue was never run"
    );

    address
}

async fn fresh(named: &str) -> (Db, Queue) {
    let address = postgres().expect("checked by the caller");
    let named = format!("mavi_work_{named}_{}", Uuid::now_v7().simple());

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

    (db, Queue::of(&[SEND, SWEEP]))
}

async fn add(db: &Db, queue: &Queue, kind: &str) -> Uuid {
    let mut tx = db.begin().await.expect("a transaction");
    let id = queue
        .add(&mut tx, kind, &Nothing {}, None)
        .await
        .expect("queued");
    tx.commit().await.expect("committed");

    id
}

/// Makes a claim lapse without waiting five minutes for it.
async fn lease_ran_out(db: &Db, job: Uuid) {
    sqlx::query("update jobs set claimed_until = now() - interval '1 second' where id = $1")
        .bind(job)
        .execute(db.pool())
        .await
        .expect("a lapsed lease");
}

#[tokio::test]
async fn two_workers_reaching_at_once_take_different_rows() {
    if postgres().is_none() {
        return;
    }

    let (db, queue) = fresh("two").await;

    let first = add(&db, &queue, SEND.name).await;
    let second = add(&db, &queue, SEND.name).await;

    let kinds = vec![SEND.name.to_owned()];

    // Both at once, on their own connections, which is the only arrangement
    // that says anything: run one after the other and `skip locked` never has
    // to do its job.
    let (one, two) = tokio::join!(
        queue.take(&db, "worker-one", &kinds),
        queue.take(&db, "worker-two", &kinds),
    );

    let one = one.expect("a claim").expect("a job");
    let two = two.expect("a claim").expect("a job");

    assert_ne!(one.id, two.id, "one row was handed to two workers");
    assert!([first, second].contains(&one.id));
    assert!([first, second].contains(&two.id));

    // And a third finds nothing rather than waiting behind either of them.
    assert!(
        queue
            .take(&db, "worker-three", &kinds)
            .await
            .expect("an answer")
            .is_none()
    );
}

#[tokio::test]
async fn a_worker_only_takes_the_kinds_it_runs() {
    if postgres().is_none() {
        return;
    }

    let (db, queue) = fresh("kinds").await;

    add(&db, &queue, SWEEP.name).await;

    assert!(
        queue
            .take(&db, "sends-letters", &[SEND.name.to_owned()])
            .await
            .expect("an answer")
            .is_none(),
        "a worker took work it does not know how to do"
    );
}

#[tokio::test]
async fn a_kind_nothing_runs_cannot_be_queued() {
    if postgres().is_none() {
        return;
    }

    let (db, queue) = fresh("unknown").await;
    let mut tx = db.begin().await.expect("a transaction");

    // What this prevents is not somebody typing badly. It is a handler being
    // renamed while something else still queues the old name — and a row that
    // nothing will ever take, in a table nothing ever empties.
    let refused = queue
        .add(&mut tx, "mail.dispatch", &Nothing {}, None)
        .await
        .expect_err("a refusal");

    assert_eq!(
        refused.said().expect("a sentence").key,
        mavi_work::NOTHING_RUNS_WORK_LIKE_THAT
    );
}

#[tokio::test]
async fn work_queued_beside_a_change_that_rolled_back_never_happened() {
    if postgres().is_none() {
        return;
    }

    let (db, queue) = fresh("rollback").await;

    {
        let mut tx = db.begin().await.expect("a transaction");
        queue
            .add(&mut tx, SEND.name, &Nothing {}, None)
            .await
            .expect("queued");
        // Dropped without committing, which is what a refusal half way
        // through a handler looks like.
    }

    let waiting: i64 = sqlx::query("select count(*) from jobs")
        .fetch_one(db.pool())
        .await
        .expect("a count")
        .get(0);

    assert_eq!(waiting, 0, "a letter went out about an order nobody placed");
}

#[tokio::test]
async fn a_job_whose_worker_died_is_taken_by_somebody_else() {
    if postgres().is_none() {
        return;
    }

    let (db, queue) = fresh("lease").await;

    let job = add(&db, &queue, SEND.name).await;
    let kinds = vec![SEND.name.to_owned()];

    let first = queue
        .take(&db, "the-one-that-dies", &kinds)
        .await
        .expect("a claim")
        .expect("a job");
    assert_eq!(first.tries, 1);

    // Nothing else may have it while the claim stands.
    assert!(
        queue
            .take(&db, "the-patient-one", &kinds)
            .await
            .expect("an answer")
            .is_none()
    );

    lease_ran_out(&db, job).await;

    let second = queue
        .take(&db, "the-patient-one", &kinds)
        .await
        .expect("a claim")
        .expect("a job");

    assert_eq!(second.id, job);
    assert_eq!(second.tries, 2, "the second run says it is the second");
}

#[tokio::test]
async fn a_worker_whose_lease_lapsed_cannot_finish_somebody_elses_job() {
    if postgres().is_none() {
        return;
    }

    let (db, queue) = fresh("stale").await;

    let job = add(&db, &queue, SEND.name).await;
    let kinds = vec![SEND.name.to_owned()];

    let slow = queue
        .take(&db, "the-slow-one", &kinds)
        .await
        .expect("a claim")
        .expect("a job");

    lease_ran_out(&db, job).await;

    let quick = queue
        .take(&db, "the-quick-one", &kinds)
        .await
        .expect("a claim")
        .expect("a job");
    assert_eq!(quick.id, slow.id);

    // The slow one finally finishes. The queue this replaces would have taken
    // this: it asked only for the id. The quick one's failure would then have
    // put the row back to ready and the work would have run a third time.
    assert_eq!(
        queue
            .done(&db, slow.id, "the-slow-one")
            .await
            .expect("an answer"),
        Ended::SomebodyElses
    );

    let state: String = sqlx::query("select state from jobs where id = $1")
        .bind(job)
        .fetch_one(db.pool())
        .await
        .expect("the row")
        .get("state");

    assert_eq!(state, "running", "somebody else's job was marked done");

    assert_eq!(
        queue
            .done(&db, quick.id, "the-quick-one")
            .await
            .expect("an answer"),
        Ended::Theirs
    );
}

#[tokio::test]
async fn work_that_keeps_failing_stops_rather_than_going_round_for_ever() {
    if postgres().is_none() {
        return;
    }

    let (db, queue) = fresh("dead").await;

    add(&db, &queue, SWEEP.name).await;
    let kinds = vec![SWEEP.name.to_owned()];

    // `forms.sweep` is worth trying once.
    let job = queue
        .take(&db, "the-worker", &kinds)
        .await
        .expect("a claim")
        .expect("a job");

    assert_eq!(
        queue
            .failed(&db, &job, "the-worker", "the sweep found a table on fire")
            .await
            .expect("an answer"),
        Ended::Theirs
    );

    let (state, went_wrong): (String, Option<String>) =
        sqlx::query("select state, went_wrong from jobs where id = $1")
            .bind(job.id)
            .fetch_one(db.pool())
            .await
            .map(|row| (row.get("state"), row.get("went_wrong")))
            .expect("the row");

    assert_eq!(state, "dead");
    assert_eq!(
        went_wrong.as_deref(),
        Some("the sweep found a table on fire"),
        "what failed and why is the thing somebody asks about afterwards"
    );

    assert!(
        queue
            .take(&db, "the-worker", &kinds)
            .await
            .expect("an answer")
            .is_none(),
        "work that had given up came round again"
    );
}

#[tokio::test]
async fn work_that_failed_once_waits_before_it_comes_round() {
    if postgres().is_none() {
        return;
    }

    let (db, queue) = fresh("again").await;

    add(&db, &queue, SEND.name).await;
    let kinds = vec![SEND.name.to_owned()];

    let job = queue
        .take(&db, "the-worker", &kinds)
        .await
        .expect("a claim")
        .expect("a job");

    queue
        .failed(&db, &job, "the-worker", "the mail host said no")
        .await
        .expect("an answer");

    // Ready again, and not yet. Asking a host that is down to try again in the
    // same second is the shape that turns one failure into a thousand.
    let due: bool =
        sqlx::query("select run_at > now() and state = 'ready' from jobs where id = $1")
            .bind(job.id)
            .fetch_one(db.pool())
            .await
            .expect("the row")
            .get(0);

    assert!(due, "a failure came straight back round");
    assert!(
        queue
            .take(&db, "the-worker", &kinds)
            .await
            .expect("an answer")
            .is_none()
    );
}
