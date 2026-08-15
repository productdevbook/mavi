#![allow(dead_code)]

pub mod queries;

use mavi::kernel::db::Db;

/// A machine of this test's own, and its log on this test's own stderr.
///
/// Every test gets one: an installation is one site, so a test that makes a
/// site has made the whole machine, and two of them cannot be the same
/// database.
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
