//! What a site writes.
//!
//! One thing with a title, a body and an address, whatever it is called: a
//! post, a page, a course, a property. The kind is a column, so a site that
//! invents a kind on Tuesday does not need a migration on Wednesday, and the
//! fields that kind carries live in `fields` rather than in a table of its
//! own.
//!
//! This crate is a library. It takes a transaction and answers; it does not
//! serve HTTP, and it does not know what a request is. What it also carries is
//! the [`endpoints`] its work is reachable through — declared here, beside the
//! functions, so that a description generated from them cannot drift from what
//! they do.

pub mod described;
pub mod kinds;
pub mod listing;
pub mod store;
pub mod writing;

use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who};
use mavi_core::error::Code;

pub use listing::{BY_FEED, BY_RECENT, Filter};
pub use writing::{Kind, New, State, Writing};

use mavi_core::grant::{Access, Needs};

/// What holding `content` is about.
pub const CONTENT: &str = "content";

#[must_use]
pub const fn to_read() -> Needs {
    Needs::new(CONTENT, Access::View)
}

#[must_use]
pub const fn to_write() -> Needs {
    Needs::new(CONTENT, Access::Write)
}

/// Everything this domain answers, said completely enough to describe.
#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/kinds",
            named: "kinds.list",
            about: "What a site decided its kinds of writing are, and what each \
                    one asks for.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::With("KindList"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Put,
            path: "/api/kinds/{kind}",
            named: "kinds.declare",
            about: "Says what a kind asks for, or says it differently. One \
                    door for both, so the checking cannot be two things.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("kind", Is::Text, "Which kind.")],
            takes: Some("Declaring"),
            answers: Answers::With("AKind"),
            refuses: &[Code::Invalid],
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/kinds/{kind}",
            named: "kinds.stop-saying",
            about: "Stops saying what a kind is. The writings stay, and so \
                    does what is in their fields.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("kind", Is::Text, "Which kind.")],
            takes: None,
            answers: Answers::Nothing,
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/writings",
            named: "writings.list",
            about: "What the site has written, newest first.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::query("kind", Is::Text, "Only this kind of thing."),
                Parameter::query("language", Is::Text, "Only what is written in this."),
                Parameter::query(
                    "state",
                    Is::Text,
                    "`draft` or `published`. Both, where it is not said.",
                ),
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("WritingPage"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/writings/{id}",
            named: "writings.read",
            about: "One of them, whatever kind it is.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which one.")],
            takes: None,
            answers: Answers::With("Writing"),
            refuses: &[Code::NotFound],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/writings",
            named: "writings.write",
            about: "Writes one.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("NewWriting"),
            answers: Answers::Made("Writing"),
            // The address is taken, or the kind does not declare a field that
            // was given. Both are things the caller can do differently, so
            // both are said rather than answered as "something went wrong".
            refuses: &[Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Patch,
            path: "/api/writings/{id}",
            named: "writings.change",
            about: "Changes one.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which one.")],
            takes: Some("WritingChanges"),
            answers: Answers::With("Writing"),
            refuses: &[Code::NotFound, Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/writings/{id}",
            named: "writings.throw-away",
            about: "Puts one in the bin. Its address is free again; it is not.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which one.")],
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
        // The assertion #11 exists for, made where it can be kept: a domain
        // that adds an endpoint and forgets to say what its path parameter is,
        // or what it refuses with, fails here rather than in somebody's
        // generated client.
        let holes = Api::of(endpoints()).holes();

        assert!(holes.is_empty(), "{holes:#?}");
    }

    #[test]
    fn nothing_reachable_is_nameless() {
        // A generator makes its method names from these, so two endpoints
        // sharing one is two methods that overwrite each other.
        let mut named: Vec<&str> = endpoints().iter().map(|e| e.named).collect();
        let all = named.len();
        named.sort_unstable();
        named.dedup();

        assert_eq!(named.len(), all, "two endpoints share a name");
    }
}
