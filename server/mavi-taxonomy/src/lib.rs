//! What a writing is filed under.
//!
//! Categories and tags are one thing with a `sort`, because a writing's
//! relationship to either is the same relationship. What actually differs is
//! one rule — a category may have a parent, a tag may not — and a rule is a
//! constraint rather than a second table.

pub mod store;
pub mod term;

use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who};
use mavi_core::error::Code;
use mavi_core::grant::{Access, Needs};
use mavi_core::page::{Key, Keyset, Kind};

pub use term::{Sort, Term};

/// What holding `taxonomy` is about.
pub const TAXONOMY: &str = "taxonomy";

#[must_use]
pub const fn to_read() -> Needs {
    Needs::new(TAXONOMY, Access::View)
}

#[must_use]
pub const fn to_write() -> Needs {
    Needs::new(TAXONOMY, Access::Write)
}

/// The panel's order, and the index in the schema matches it column for
/// column. An index that does not match the order is one the planner ignores,
/// and nothing reports it because the answer is still correct.
pub const BY_RECENT: Keyset = Keyset(&[
    Key::newest("created_at", Kind::Moment),
    Key::newest("id", Kind::Id),
]);

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/terms",
            named: "terms.list",
            about: "What this site files things under.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::query("sort", Is::Text, "`category` or `tag`. Both, unsaid."),
                Parameter::query("language", Is::Text, "Only the ones written in this."),
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("TermPage"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/terms",
            named: "terms.make",
            about: "Makes one.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("NewTerm"),
            answers: Answers::Made("Term"),
            // The address is taken, or a tag was given a parent, or a parent
            // would put a category under itself.
            refuses: &[Code::Conflict, Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Patch,
            path: "/api/terms/{id}",
            named: "terms.change",
            about: "Renames one, or moves it under another.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which one.")],
            takes: Some("TermChanges"),
            answers: Answers::With("Term"),
            refuses: &[Code::NotFound, Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/terms/{id}",
            named: "terms.remove",
            about: "Removes one. What was filed under it stays, filed under nothing.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which one.")],
            takes: None,
            answers: Answers::Nothing,
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Put,
            path: "/api/writings/{id}/terms",
            named: "writings.file-under",
            about: "Says what one writing is filed under. Replaces whatever it was.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which writing.")],
            takes: Some("Filing"),
            // A list, because it answers what it is filed under now — and the
            // shape it declares is the shape it answers with, which is the one
            // thing an endpoint in the crate this replaces got wrong.
            answers: Answers::With("TermList"),
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
    fn the_order_ends_with_something_unique() {
        assert_eq!(
            BY_RECENT.keys().last().expect("a key").column,
            "id",
            "an order that cannot break a tie"
        );
    }

    #[test]
    fn what_this_domain_asks_for_is_a_capability_the_site_has() {
        assert!(mavi_people::is_a_capability(TAXONOMY));
    }
}
