//! The description, written out where the panel reads it.
//!
//! Committed rather than built, and this test is what keeps it true. It writes
//! the file and then compares it with what was there before — so a shape that
//! changed and a panel that has not caught up is a red build here, with the
//! corrected file already sitting in the tree, rather than a screen that
//! breaks in somebody's browser.
//!
//! Writing before comparing rather than after is deliberate: the thing
//! somebody needs when this fails is the new file, and a test that only says
//! "these differ" leaves them working out how to produce it.

use std::path::PathBuf;

/// Where the panel keeps what it was given.
fn at() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../client/src/api/mavi.ts")
}

#[test]
fn the_panel_has_what_the_api_says_it_is() {
    let written = mavi_api::typescript(&mavi_everything::api());
    let at = at();

    let before = std::fs::read_to_string(&at).ok();

    if let Some(parent) = at.parent() {
        std::fs::create_dir_all(parent).expect("somewhere to put it");
    }

    std::fs::write(&at, &written).expect("written");

    let Some(before) = before else {
        panic!(
            "the panel had no types. They have been written to {} — commit them.",
            at.display()
        );
    };

    assert_eq!(
        before,
        written,
        "the panel's types and the API had come apart. The new ones have been \
         written to {} — commit them.",
        at.display()
    );
}
