#![allow(dead_code)]

pub mod queries;

use std::fmt::Write as _;

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
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }

    out
}
