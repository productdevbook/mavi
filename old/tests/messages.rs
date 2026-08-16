//! What somebody is told is a key, and the English lives in one file.
//!
//! Checked by reading the source, because there is no type that can say "this
//! string was written at the place it is refused". A sentence written into a
//! handler is a sentence no panel can show in anybody else's language, and the
//! only moment to catch it is before it is merged.

use std::path::Path;

/// Everything a refusal is made of. A fourth would have to be added here as
/// well as to the error type, which is the point.
const CARRY_A_MESSAGE: [&str; 3] = [
    "AppError::Invalid(",
    "AppError::Refused(",
    "AppError::Conflict(",
];

#[test]
fn nothing_is_told_to_anybody_in_english_at_the_place_it_is_refused() {
    for file in rust_files(Path::new("src")) {
        let source = std::fs::read_to_string(&file).expect("a source file");

        // say.rs is where the English belongs, and error.rs is where the type
        // is; both mention the same words for reasons that are not this.
        if file.ends_with("kernel/say.rs") || file.ends_with("kernel/error.rs") {
            continue;
        }

        for (number, line) in source.lines().enumerate() {
            for start in CARRY_A_MESSAGE {
                let Some(after) = line.split_once(start).map(|(_, after)| after.trim_start())
                else {
                    continue;
                };

                let said_here = after.starts_with('"') || after.starts_with("format!");

                assert!(
                    !said_here,
                    "{}:{} says something in English where it is refused; \
                     give it a key in kernel::say instead",
                    file.display(),
                    number + 1
                );
            }
        }
    }
}

#[test]
fn every_key_there_is_says_something_in_english() {
    let source = std::fs::read_to_string("src/kernel/say.rs").expect("the catalogue");

    // Read from the whole file rather than line by line: the formatter wraps a
    // long enough declaration onto two lines, and a check that misses those is
    // a check that passes because it looked at less.
    let declared: Vec<String> = source
        .split("pub const ")
        .skip(1)
        .filter_map(|rest| rest.split_once("&str ="))
        .filter_map(|(_, value)| value.split('"').nth(1))
        .map(str::to_owned)
        .collect();

    assert!(
        declared.len() > 50,
        "the catalogue was read wrong: {} keys",
        declared.len()
    );

    for key in &declared {
        assert!(
            mavi::kernel::say::english(key).is_some(),
            "{key} is a key nothing says"
        );
    }

    for (key, _) in mavi::kernel::say::ENGLISH {
        assert!(
            declared.iter().any(|declared| declared == key),
            "{key} says something and is not a key anything can use"
        );
    }
}

#[test]
fn a_refusal_carries_the_key_as_well_as_the_english() {
    use mavi::kernel::error::AppError;
    use mavi::kernel::say::{self, Say};

    let refused = AppError::Invalid(Say::of(say::NOT_THAT_MANY_LEFT).naming("name", "a chair"));

    let said = refused.said().expect("a refusal says something");

    assert_eq!(said.key(), "not_that_many_left");
    assert_eq!(said.named(), [("name", "a chair".to_owned())]);
    assert_eq!(refused.to_string(), "there are not that many a chair left");
}

fn rust_files(at: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();

    for entry in std::fs::read_dir(at).expect("a directory") {
        let path = entry.expect("an entry").path();

        if path.is_dir() {
            found.extend(rust_files(&path));
        } else if path.extension().is_some_and(|kind| kind == "rs") {
            found.push(path);
        }
    }

    found
}

/// A `warn!(?body)` on a handler that takes a password would put it in the log,
/// and the log is where a secret is hardest to take back out of. So nothing
/// that carries one holds it as a plain `String`.
#[test]
fn nothing_that_carries_a_secret_can_print_itself() {
    const A_SECRET: [&str; 5] = [
        "pub password: String",
        "pub secret: String",
        "pub token: String",
        "pub client_secret: String",
        "pub signing: String",
    ];

    for file in rust_files(Path::new("src")) {
        let source = std::fs::read_to_string(&file).expect("a source file");

        for (number, line) in source.lines().enumerate() {
            for held_plainly in A_SECRET {
                assert!(
                    !line.trim_end_matches(',').ends_with(held_plainly),
                    "{}:{} holds a secret as a plain string; `Secret` for one \
                     coming in, `Shown` for one going out",
                    file.display(),
                    number + 1
                );
            }
        }
    }
}

#[test]
fn a_secret_says_nothing_when_it_is_printed() {
    use mavi::kernel::secret::{Secret, Shown};

    let coming_in = Secret::new("a password this test invented".to_owned());
    let going_out = Shown::new("a token this test invented".to_owned());

    assert!(!format!("{coming_in:?}").contains("invented"));
    assert!(!format!("{coming_in}").contains("invented"));
    assert!(!format!("{going_out:?}").contains("invented"));

    // And what goes out still goes out: the whole point of the second one.
    assert_eq!(
        serde_json::to_string(&going_out).expect("json"),
        "\"a token this test invented\""
    );
}
