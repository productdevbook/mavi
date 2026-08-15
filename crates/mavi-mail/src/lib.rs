//! What a site writes to people.
//!
//! Three things: what its own letters say, who is on its lists, and what it
//! sends them. What actually puts a letter on the wire is not here and is not
//! written yet — that belongs with the queue, because a letter that fails to
//! send has to be tried again and nothing here can hold that. This crate is
//! the shape of a site's mail; the sending is what will read it.
//!
//! The one decision everything else follows from: a letter answering something
//! somebody did and a letter to a list are different things, not one thing
//! with a flag. See [`who::Purpose`].

pub mod letter;
pub mod sending;
pub mod who;

use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who};
use mavi_core::error::Code;
use mavi_core::grant::{Access, Needs};
use mavi_core::id;
use mavi_core::page::{Key, Keyset, Kind};

pub use letter::{Pressed, Wording, press};
pub use sending::Sending;
pub use who::{Purpose, Standing, may_write};

id!(
    /// One list somebody can be on.
    ListId
);

id!(
    /// One person a site writes to.
    ReaderId
);

pub const MAIL: &str = "mail";

#[must_use]
pub const fn to_read() -> Needs {
    Needs::new(MAIL, Access::View)
}

#[must_use]
pub const fn to_write() -> Needs {
    Needs::new(MAIL, Access::Write)
}

/// The panel's order over readers, and the index matches it column for column.
pub const BY_RECENT: Keyset = Keyset(&[
    Key::newest("created_at", Kind::Moment),
    Key::newest("id", Kind::Id),
]);

/// The order over one list's readers.
///
/// `reader_id` rather than `id`, because the row being walked is the pairing
/// and not the reader — a keyset naming a column the query does not select is
/// a cursor that decodes and then finds nothing.
pub const ON_A_LIST: Keyset = Keyset(&[
    Key::newest("created_at", Kind::Moment),
    Key::newest("reader_id", Kind::Id),
]);

/// What the link at the bottom of a letter carries.
///
/// The same minting as an invitation or a password link: what goes in the
/// letter is shown once and what is kept is its hash. A way out is a key to
/// somebody's standing with this site, and a link sitting in an inbox is a
/// link in whatever else reads that inbox.
#[must_use]
pub fn way_out() -> mavi_people::token::Minted {
    mavi_people::token::mint()
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    let mut all = the_letters();
    all.extend(the_lists());
    all.extend(for_anybody());
    all
}

/// What the site's own letters say. One per kind and language, and a kind
/// nothing sends cannot be written for.
fn the_letters() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/mail/letters",
            named: "letters.list",
            about: "Every letter this site sends, and whether the wording is its own.",
            who: Who::AnAccount,
            parameters: vec![Parameter::query(
                "language",
                Is::Text,
                "The wording written in this. The site's own, unsaid.",
            )],
            takes: None,
            answers: Answers::With("LetterList"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Put,
            path: "/api/mail/letters/{kind}",
            named: "letters.write",
            about: "Says what one letter says, in one language.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("kind", Is::Text, "Which letter.")],
            takes: Some("Wording"),
            answers: Answers::With("Letter"),
            // A kind nothing sends, or wording that names something the letter
            // does not have.
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/mail/letters/{kind}",
            named: "letters.forget",
            about: "Goes back to this machine's own wording for one letter.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::path("kind", Is::Text, "Which letter."),
                Parameter::query("language", Is::Text, "Which language's wording."),
            ],
            takes: None,
            answers: Answers::Nothing,
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/mail/letters/{kind}/pressed",
            named: "letters.press",
            about: "What one letter looks like filled in, without sending it.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("kind", Is::Text, "Which letter.")],
            takes: Some("Values"),
            answers: Answers::With("Pressed"),
            refuses: &[Code::NotFound],
            // Reads. A `POST` because it carries what to fill in, and this is
            // the one place that distinction is made — never by the verb.
            changes: false,
        },
    ]
}

/// Who is on the site's lists, and what it says to them.
fn the_lists() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/mail/lists",
            named: "lists.list",
            about: "The site's lists, and how many are on each.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::With("ListList"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/mail/lists",
            named: "lists.make",
            about: "Makes one.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("NewList"),
            answers: Answers::Made("List"),
            refuses: &[Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/mail/lists/{id}/readers",
            named: "readers.list",
            about: "Who is on one list, newest first.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::path("id", Is::Id, "Which list."),
                Parameter::query("standing", Is::Text, "Only those standing like this."),
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("ReaderPage"),
            refuses: &[Code::NotFound],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/mail/lists/{id}/readers",
            named: "readers.add",
            about: "Puts somebody on a list.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which list.")],
            takes: Some("NewReader"),
            answers: Answers::Made("Reader"),
            refuses: &[Code::NotFound, Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/mail/readers/{id}",
            named: "readers.forget",
            about: "Forgets somebody entirely, lists and all.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which reader.")],
            takes: None,
            answers: Answers::Nothing,
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/mail/lists/{id}/sendings",
            named: "sendings.send",
            about: "Says something to a list. Refused unless it says how to leave it.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which list.")],
            takes: Some("Sending"),
            // The queue takes it from here, which is why this answers 202 and
            // not a letter.
            answers: Answers::Later,
            refuses: &[Code::NotFound],
            changes: true,
        },
    ]
}

/// What anybody reaches, which is one thing: the way out.
fn for_anybody() -> Vec<Endpoint> {
    vec![Endpoint {
        method: Method::Post,
        path: "/api/open/mail/out/{token}",
        named: "open.unsubscribe",
        about: "Takes somebody off a list. The link at the bottom of a letter.",
        who: Who::Anybody,
        parameters: vec![Parameter::path(
            "token",
            Is::Text,
            "What the link in their letter carried.",
        )],
        takes: None,
        // Answers the same whether the token was theirs or was never anything,
        // because a link in an inbox is a link in whatever reads that inbox,
        // and the difference is a way to ask who is on this site's lists.
        answers: Answers::Nothing,
        refuses: &[],
        changes: true,
    }]
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
    fn no_two_of_these_are_the_same_route() {
        let clashes = Api::of(endpoints()).clashes();

        assert!(clashes.is_empty(), "{clashes:#?}");
    }

    #[test]
    fn what_anybody_can_reach_says_so_in_its_path() {
        for endpoint in endpoints() {
            assert_eq!(
                endpoint.who == Who::Anybody,
                endpoint.path.starts_with("/api/open/"),
                "{} is one thing in its path and another in its audience",
                endpoint.named
            );
        }
    }

    #[test]
    fn the_way_out_is_reachable_without_signing_in() {
        // A way out that needs an account is not a way out: whoever wants off
        // a list is exactly whoever never made one.
        let out = endpoints()
            .into_iter()
            .find(|e| e.named == "open.unsubscribe")
            .expect("a way out");

        assert_eq!(out.who, Who::Anybody);
    }

    #[test]
    fn what_this_domain_asks_for_is_a_capability_the_site_has() {
        assert!(mavi_people::is_a_capability(MAIL));
    }

    #[test]
    fn every_order_here_ends_with_something_unique() {
        for keyset in [BY_RECENT, ON_A_LIST] {
            let last = keyset.keys().last().expect("a key").column;

            assert!(
                last.ends_with("id"),
                "{last} cannot break a tie between two rows written in one transaction"
            );
        }
    }

    #[test]
    fn two_ways_out_are_two_different_ways_out() {
        assert_ne!(way_out().token, way_out().token);
    }
}
