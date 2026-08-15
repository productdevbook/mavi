#![allow(dead_code)]

pub mod queries;

use mavi::kernel::db::Db;
use sqlx::Connection as _;
use uuid::Uuid;

/// Set up once under an advisory lock, because several test binaries reach the
/// same database at the same time.
pub async fn harness() -> Db {
    // What a failing request logged, on the test's own stderr: the body a
    // caller gets says nothing about the database on purpose, so without this
    // a five hundred is a number and nothing else.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_test_writer()
        .try_init();

    mavi::testing::harness().await
}

/// A database nothing else is using, for the handful of tests about things
/// that are the machine's rather than a site's — who runs it, and whether it
/// has been set up. Those live in one global table, so a test that asserts
/// "there is nobody yet" cannot share a database with a test that makes
/// somebody.
pub async fn a_machine_of_its_own() -> Db {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let name = format!("v1_alone_{}", Uuid::now_v7().simple());

    // On a connection of its own rather than through `Db`: making a database
    // is the one thing Postgres refuses to do inside a transaction, and every
    // handle this codebase hands out is already in one.
    let mut making = sqlx::PgConnection::connect(&url).await.expect("connect");

    sqlx::query(&format!("create database {name}"))
        .execute(&mut making)
        .await
        .expect("a database of its own");

    let (before, _) = url.rsplit_once('/').expect("a database in the url");
    let its_own = format!("{before}/{name}");

    let db = Db::connect(&its_own, 4).await.expect("connect");
    db.migrate().await.expect("migrate");

    db
}

/// A cursor handed back by a page carries `:` and the like, which a query
/// string cannot: this is what a client following `next` does before asking
/// for it.
#[must_use]
pub fn percent_encoded(value: &str) -> String {
    let mut out = String::with_capacity(value.len());

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }

    out
}
