//! The description, written out where the panel reads it.
//!
//! Committed rather than built, and this test is what keeps it true: it writes
//! the file when told to and compares it otherwise. So a shape that changed
//! and a panel that has not caught up is a red build here, rather than a
//! screen that breaks in somebody's browser.
//!
//!     UPDATE_TYPES=1 cargo test -p mavi-everything --test described

use std::path::PathBuf;

/// Where the panel keeps what it was given.
fn at() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../client/src/api/mavi.ts")
        .canonicalize()
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../client/src/api/mavi.ts")
        })
}

#[test]
fn the_panel_has_what_the_api_says_it_is() {
    let written = mavi_api::typescript(&mavi_everything::api());

    if std::env::var("UPDATE_TYPES").is_ok() {
        let at = at();

        if let Some(parent) = at.parent() {
            std::fs::create_dir_all(parent).expect("somewhere to put it");
        }

        std::fs::write(&at, &written).expect("written");

        return;
    }

    let there = std::fs::read_to_string(at()).expect(
        "the panel's types are missing — write them with \
         `UPDATE_TYPES=1 cargo test -p mavi-everything --test described`",
    );

    assert_eq!(
        there, written,
        "the panel's types and the API have come apart — write them again with \
         `UPDATE_TYPES=1 cargo test -p mavi-everything --test described`"
    );
}
