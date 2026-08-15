//! What somebody uploaded.
//!
//! A file is kept under its id, not under the name it arrived with, and what
//! it is comes from its bytes rather than from that name. Both of those are
//! failures the crate this replaces had a test for, and both are worth keeping
//! as rules rather than as tests.

pub mod kept;

use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who};
use mavi_core::error::Code;
use mavi_core::grant::{Access, Needs};
use mavi_core::page::{Key, Keyset, Kind};

pub use kept::{FileId, Looked, kept_at, look};

pub const MEDIA: &str = "media";

#[must_use]
pub const fn to_read() -> Needs {
    Needs::new(MEDIA, Access::View)
}

#[must_use]
pub const fn to_write() -> Needs {
    Needs::new(MEDIA, Access::Write)
}

pub const BY_RECENT: Keyset = Keyset(&[
    Key::newest("created_at", Kind::Moment),
    Key::newest("id", Kind::Id),
]);

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/files",
            named: "files.list",
            about: "What has been uploaded, newest first.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::query("kind", Is::Text, "`image`, `video`, `audio` or `document`."),
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("FilePage"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/files",
            named: "files.upload",
            about: "Takes a file. What it is is read from the bytes, not the name.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::query(
                    "name",
                    Is::Text,
                    "What to call it when showing it back. Never where it is kept.",
                )
                .required(),
            ],
            // The bytes themselves, raw. Said rather than left out: the crate
            // this replaces read a body its description never mentioned, which
            // is the single hardest wall in an API for somebody writing a
            // client.
            takes: Some("TheBytes"),
            answers: Answers::Made("File"),
            refuses: &[],
            changes: true,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/files/{id}",
            named: "files.read",
            about: "One file's details. The bytes are served from where it lives.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which file.")],
            takes: None,
            answers: Answers::With("File"),
            refuses: &[Code::NotFound],
            changes: false,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/files/{id}",
            named: "files.remove",
            about: "Removes one, and the bytes with it.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which file.")],
            takes: None,
            answers: Answers::Nothing,
            refuses: &[Code::NotFound],
            changes: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_api::Api;

    #[test]
    fn everything_this_domain_answers_is_described_completely() {
        let holes = Api::of(endpoints()).holes();

        assert!(holes.is_empty(), "{holes:#?}");
    }

    #[test]
    fn the_endpoint_that_reads_a_body_says_it_reads_one() {
        // The crate this replaces took raw bytes with the name in the query
        // and declared neither, so a generated client called it with an empty
        // body and was told the file was empty.
        let upload = endpoints()
            .into_iter()
            .find(|e| e.named == "files.upload")
            .expect("an upload");

        assert!(
            upload.takes.is_some(),
            "it reads a body and does not say so"
        );
        assert!(
            upload.parameters.iter().any(|p| p.name == "name"),
            "it reads a query parameter and does not say so"
        );
    }

    #[test]
    fn the_order_ends_with_something_unique() {
        assert_eq!(
            BY_RECENT.keys().last().expect("a key").column,
            "id",
            "an order that cannot break a tie"
        );
    }

    #[test]
    fn what_this_domain_asks_for_is_a_capability_the_site_has() {
        assert!(mavi_people::is_a_capability(MEDIA));
    }
}
