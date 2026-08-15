//! The record, against a real Postgres.
//!
//! Two of the three things this crate claims can only be shown by running it:
//! that a receipt written beside a change that rolls back does not exist, and
//! that a receipt already written cannot be altered by anybody at all —
//! including by the code that wrote it. The third, that a receipt cannot be
//! made without writing a row, is the type system's and needs no test.

use mavi_audit::{Actor, Who, record};
use mavi_db::Db;
use sqlx::{Connection, PgConnection, Row};
use uuid::Uuid;

fn postgres() -> Option<String> {
    let address = std::env::var("TEST_DATABASE_URL").ok();

    assert!(
        address.is_some() || std::env::var("CI").is_err(),
        "CI has no TEST_DATABASE_URL, so the record was never written"
    );

    address
}

async fn fresh(named: &str) -> Db {
    let address = postgres().expect("checked by the caller");
    let named = format!("mavi_audit_{named}_{}", Uuid::now_v7().simple());

    let mut admin = PgConnection::connect(&address).await.expect("a connection");
    sqlx::query(&format!("create database {named}"))
        .execute(&mut admin)
        .await
        .expect("a database of its own");

    let (front, _) = address
        .rsplit_once('/')
        .expect("an address with a database");
    let db = Db::open(&format!("{front}/{named}"), 2)
        .await
        .expect("the new database");

    db.migrate().await.expect("every migration");

    db
}

fn an_editor() -> Actor {
    Actor {
        who: Who::AnAccount,
        id: Some("01930000-0000-7000-8000-000000000001".to_owned()),
        request: "one-request".to_owned(),
    }
}

#[tokio::test]
async fn a_receipt_says_what_was_done_and_by_whom() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("written").await;
    let mut tx = db.begin().await.expect("a transaction");

    let receipt = record(
        &mut tx,
        &an_editor(),
        "writings.publish",
        "writing",
        Some("01930000-0000-7000-8000-00000000000a"),
        &serde_json::json!({ "from": "draft", "to": "published" }),
    )
    .await
    .expect("a receipt");

    tx.commit().await.expect("committed");

    let (did, what): (String, serde_json::Value) =
        sqlx::query("select did, what from receipts where id = $1")
            .bind(receipt.wrote)
            .fetch_one(db.pool())
            .await
            .map(|row| (row.get("did"), row.get("what")))
            .expect("the row");

    assert_eq!(did, "writings.publish");
    assert_eq!(what["from"], "draft");
}

#[tokio::test]
async fn a_change_that_rolled_back_left_no_receipt() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("rollback").await;

    {
        let mut tx = db.begin().await.expect("a transaction");
        record(
            &mut tx,
            &an_editor(),
            "writings.publish",
            "writing",
            None,
            &serde_json::json!({}),
        )
        .await
        .expect("a receipt");
        // Dropped without committing: the change was refused half way through,
        // so it never happened and neither did the record of it.
    }

    let written: i64 = sqlx::query("select count(*) from receipts")
        .fetch_one(db.pool())
        .await
        .expect("a count")
        .get(0);

    assert_eq!(written, 0, "a receipt for something that never happened");
}

#[tokio::test]
async fn a_receipt_is_written_once_and_nobody_can_change_it_afterwards() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("once").await;
    let mut tx = db.begin().await.expect("a transaction");

    let receipt = record(
        &mut tx,
        &an_editor(),
        "people.remove",
        "person",
        Some("01930000-0000-7000-8000-00000000000b"),
        &serde_json::json!({}),
    )
    .await
    .expect("a receipt");

    tx.commit().await.expect("committed");

    // Said to the database rather than to the code, because the code is not
    // the only thing that reaches this table.
    for wrong in [
        "update receipts set did = 'something else' where id = $1",
        "delete from receipts where id = $1",
    ] {
        let refused = sqlx::query(wrong)
            .bind(receipt.wrote)
            .execute(db.pool())
            .await
            .expect_err(wrong);

        assert!(
            refused.to_string().contains("written once"),
            "{wrong} was allowed: {refused}"
        );
    }

    let still_there: i64 = sqlx::query("select count(*) from receipts where id = $1")
        .bind(receipt.wrote)
        .fetch_one(db.pool())
        .await
        .expect("a count")
        .get(0);

    assert_eq!(still_there, 1);
}

#[tokio::test]
async fn the_machine_has_no_id_and_everybody_else_has_one() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("who").await;
    let mut tx = db.begin().await.expect("a transaction");

    // A scheduled publish is written down like anything else: "nobody did
    // this" is an answer somebody will need one day.
    record(
        &mut tx,
        &Actor::the_machine("the-scheduler"),
        "writings.publish",
        "writing",
        None,
        &serde_json::json!({ "because": "it was time" }),
    )
    .await
    .expect("a receipt");

    tx.commit().await.expect("committed");

    let half = sqlx::query(
        "insert into receipts (id, who, who_id, did, about, request)
         values ($1, 'an_account', null, 'writings.publish', 'writing', 'a-request')",
    )
    .bind(Uuid::now_v7())
    .execute(db.pool())
    .await
    .expect_err("an account with no id");

    assert!(
        half.to_string().contains("somebody_or_the_machine"),
        "a receipt attributed to nobody in particular: {half}"
    );
}
