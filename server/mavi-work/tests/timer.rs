//! One tick is one process's.
//!
//! The claim this makes is about two processes at the same moment, so it is
//! asked of two connections rather than of the function. Two workers both
//! deciding it is time to put stock back is the sweep running twice; the whole
//! design of a tick being claimed by the statement that moves it forward is
//! for exactly this test to pass.

use std::time::Duration;

use mavi_db::Db;
use mavi_work::timer::{Often, due, keep};
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

const SWEEP: Often = Often::minutes("shop.put-back", 5);
const LETTERS: Often = Often::minutes("mail.send", 1);

fn postgres() -> Option<String> {
    let address = std::env::var("TEST_DATABASE_URL").ok();

    assert!(
        address.is_some() || std::env::var("CI").is_err(),
        "CI has no TEST_DATABASE_URL, so no tick was ever claimed"
    );

    address
}

async fn fresh(named: &str) -> Db {
    let address = postgres().expect("checked by the caller");
    let named = format!(
        "mavi_timer_{}_{}",
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

async fn written_down(db: &Db, all: &[Often]) {
    let mut tx = db.begin().await.expect("a transaction");
    keep(&mut tx, all).await.expect("the schedules");
    tx.commit().await.expect("committed");
}

async fn asked(db: &Db, all: &[Often]) -> Vec<String> {
    let mut tx = db.begin().await.expect("a transaction");
    let due = due(&mut tx, all).await.expect("an answer");
    tx.commit().await.expect("committed");

    due
}

#[tokio::test]
async fn a_new_installation_does_its_first_sweep_rather_than_waiting_for_one() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("first").await;
    written_down(&db, &[SWEEP]).await;

    // `next_at` starts at now, so the first tick after a start is due. A
    // schedule that waited a whole interval first is a machine that does
    // nothing for five minutes after every rollout.
    assert_eq!(asked(&db, &[SWEEP]).await, ["shop.put-back"]);
}

#[tokio::test]
async fn asking_again_before_it_is_due_gets_nothing() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("again").await;
    written_down(&db, &[SWEEP]).await;

    assert_eq!(asked(&db, &[SWEEP]).await.len(), 1);
    assert!(asked(&db, &[SWEEP]).await.is_empty());
}

#[tokio::test]
async fn two_processes_asking_at_once_is_one_tick() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("two").await;
    written_down(&db, &[SWEEP]).await;

    // Both at the same moment, on their own transactions. One of them moves
    // the row forward; the other reads it after that commit and finds a time
    // in the future.
    let (one, two) = tokio::join!(asked(&db, &[SWEEP]), asked(&db, &[SWEEP]));

    assert_eq!(
        one.len() + two.len(),
        1,
        "the sweep was claimed twice: {one:?} {two:?}"
    );
}

#[tokio::test]
async fn what_is_due_is_only_what_this_process_runs() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("mine").await;
    written_down(&db, &[SWEEP, LETTERS]).await;

    // A worker that runs one kind does not claim the other's tick and leave
    // it moved forward with nobody having done it.
    let mine = asked(&db, &[LETTERS]).await;

    assert_eq!(mine, ["mail.send"]);
    assert_eq!(asked(&db, &[SWEEP]).await, ["shop.put-back"]);
}

#[tokio::test]
async fn a_schedule_comes_round_again_when_its_time_has_passed() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("round").await;
    written_down(&db, &[SWEEP]).await;

    assert_eq!(asked(&db, &[SWEEP]).await.len(), 1);

    // Rather than waiting five minutes for it.
    sqlx::query("update schedules set next_at = now() - interval '1 second'")
        .execute(db.pool())
        .await
        .expect("time passing");

    assert_eq!(asked(&db, &[SWEEP]).await, ["shop.put-back"]);
}

#[tokio::test]
async fn how_often_follows_the_code_rather_than_the_row() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("interval").await;
    written_down(&db, &[Often::minutes("shop.put-back", 5)]).await;

    // The interval lives in the code. A row written by a version that said
    // five minutes is corrected by a version that says one, at the next
    // start, rather than being a second copy that drifts.
    written_down(&db, &[Often::minutes("shop.put-back", 1)]).await;

    let seconds: i64 = sqlx::query_scalar("select every_seconds from schedules")
        .fetch_one(db.pool())
        .await
        .expect("the row");

    assert_eq!(seconds, 60);
    assert_eq!(Duration::from_secs(60).as_secs(), 60);
}
