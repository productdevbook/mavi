//! What a site threw away.
//!
//! Ten tables in this schema keep what was deleted rather than deleting it,
//! and until now **nothing could bring any of it back or take it away for
//! good**. So "kept" meant "invisible for ever": a table that grows, holding
//! somebody's content and somebody's uploads with no way to reach either and
//! no decision anywhere to keep them.
//!
//! That is worse than deleting outright, and not only because a mistake is
//! unrecoverable. Data held with no purpose and no way to remove it is the
//! thing whoever runs a site has to answer for.
//!
//! ## What can be reached from here, and what cannot
//!
//! Not every table with a `deleted_at` on it — see [`kind::Kind`]. What is
//! here is what somebody makes and might unmake by mistake. An order, a
//! session and a ticket are kept for reasons of their own and are not things
//! anybody restores.

pub mod described;
pub mod kind;
pub mod store;

use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who};
use mavi_core::error::Code;
use mavi_core::grant::{Access, Needs};

pub use kind::Kind;
pub use store::Thrown;

/// The most a screen may ask for at once.
pub const AT_MOST: i64 = 200;

/// What holding `trash` is about.
///
/// Its own capability rather than each domain's, and that is the decision
/// worth naming: what is in the bin is **everything** — writings, uploads,
/// products, whoever's forms — and somebody who may edit a post has not
/// thereby been given a screen listing everything anybody ever deleted.
pub const TRASH: &str = "trash";

#[must_use]
pub const fn to_read() -> Needs {
    Needs::new(TRASH, Access::View)
}

#[must_use]
pub const fn to_change() -> Needs {
    Needs::new(TRASH, Access::Write)
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/trash",
            named: "trash.list",
            about: "What a site threw away, newest first.",
            who: Who::AnAccount,
            parameters: vec![Parameter::query(
                "how_many",
                Is::Number,
                "How many, at most two hundred.",
            )],
            takes: None,
            answers: Answers::With("ThrownList"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/trash/{sort}/{id}",
            named: "trash.put-back",
            about: "Puts one back. Refused where something else has taken its \
                    address in the meantime.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::path("sort", Is::Text, "What sort of thing."),
                Parameter::path("id", Is::Id, "Which one."),
            ],
            takes: None,
            answers: Answers::Nothing,
            refuses: &[Code::NotFound, Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/trash/{sort}/{id}",
            named: "trash.for-good",
            about: "Takes one away for good.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::path("sort", Is::Text, "What sort of thing."),
                Parameter::path("id", Is::Id, "Which one."),
            ],
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
    fn what_is_in_the_bin_is_not_something_editing_a_post_shows_you() {
        // The reason this has a capability of its own. The bin is everything —
        // writings, uploads, products, whoever's forms — in one screen.
        assert_eq!(to_read().of, TRASH);
        assert_ne!(TRASH, "content");
    }
}
