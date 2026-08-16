//! What the queue may be asked to run, against what there is to run.
//!
//! `jobs.rs` proves the half of this that needs a database: every kind it
//! advertises is one `run` knows how to do. The other half is a list against a
//! list, and the list of kinds is read out of the source rather than kept by
//! hand — a copy of it here would be the thing that goes stale, and a gate
//! that goes stale is worse than no gate.

use std::collections::BTreeSet;
use std::path::Path;

/// Every `KIND` a `Task` declares, as the source says it.
fn declared() -> BTreeSet<String> {
    let mut found = BTreeSet::new();

    walk(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(),
        &mut found,
    );

    found
}

fn walk(at: &Path, found: &mut BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(at) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            walk(&path, found);
            continue;
        }

        if path.extension().is_none_or(|kind| kind != "rs") {
            continue;
        }

        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };

        for line in source.lines() {
            let line = line.trim();

            let Some(rest) = line.strip_prefix("const KIND: &'static str = \"") else {
                continue;
            };

            if let Some(kind) = rest.strip_suffix("\";") {
                found.insert(kind.to_owned());
            }
        }
    }
}

/// A kind the queue asks for that nothing declares is a worker waiting on work
/// that will never be made.
#[test]
fn every_kind_the_queue_asks_for_is_one_something_declares() {
    let declared = declared();

    for kind in mavi::jobs::kinds(&mavi::kernel::outside::Outside::default()) {
        assert!(
            declared.contains(&kind),
            "the queue asks for {kind} and no task in this build declares it"
        );
    }
}

/// And the other way, which is the one that bites: a task nobody asks for is
/// work that is queued, claimed by nothing, and sits there looking like work
/// about to happen.
#[test]
fn every_kind_a_task_declares_is_one_the_queue_asks_for() {
    let asked_for: BTreeSet<String> = mavi::jobs::kinds(&mavi::kernel::outside::Outside::default())
        .into_iter()
        .collect();

    for kind in declared() {
        assert!(
            asked_for.contains(&kind),
            "{kind} is a task's kind and no worker is ever told to claim it"
        );
    }
}
