//! Who can sign in, and what they may do.
//!
//! This crate owns the **list** of what a site can do — the capabilities a
//! grant is made of. `mavi-core` knows the shape of a grant and interprets
//! nothing; the names live here, beside the endpoints that ask for them,
//! because the alternative is a foundation edited every time a site learns to
//! do something new.

pub mod described;
pub mod owner;
pub mod password;
pub mod store;
pub mod ticket;
pub mod token;

use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who};
use mavi_core::error::Code;
use mavi_core::grant::{Access, Needs};

pub use ticket::{For, Ticket};

/// Everything a grant can be about.
///
/// One list, in one place, and the only place. A domain that wants a new one
/// adds it here rather than inventing a string at the point it checks — which
/// is the difference between a capability and a typo.
pub const CAPABILITIES: &[&str] = &[
    "content", "media", "taxonomy", "forms", "mail", "flows", "courses", "shop", "people",
    "settings", "publish", "design", "boards", "audit",
];

/// What holding `people` is about: the accounts themselves, and what they may
/// do. Named here beside the list rather than derived from it, so that
/// something asking for it is asking for a name the compiler checked.
pub const PEOPLE: &str = "people";

#[must_use]
pub const fn to_read() -> Needs {
    Needs::new(PEOPLE, Access::View)
}

#[must_use]
pub const fn to_write() -> Needs {
    Needs::new(PEOPLE, Access::Write)
}

/// Whether this is a capability at all, asked where a grant is read rather
/// than trusted because it was in the database.
#[must_use]
pub fn is_a_capability(name: &str) -> bool {
    CAPABILITIES.contains(&name)
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Post,
            path: "/api/setup",
            named: "setup.once",
            about: "Makes the site, the owner's role, and the account that holds it. Answers once.",
            who: Who::Anybody,
            parameters: Vec::new(),
            takes: Some("Setup"),
            answers: Answers::Made("Ready"),
            // Already set up. Never a way to ask whether it is: an
            // installation that has been set up refuses this and an
            // installation that has not takes it, which is the same thing a
            // visitor learns by looking at the front page.
            refuses: &[Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/sessions",
            named: "sessions.begin",
            about: "Signs somebody in, and hands back the token that says so.",
            who: Who::Anybody,
            parameters: Vec::new(),
            takes: Some("Credentials"),
            // Never `NotFound`: an address that has no account and an address
            // with the wrong password answer the same way, or the refusal is
            // a way to ask which addresses have accounts.
            refuses: &[Code::Forbidden],
            answers: Answers::Made("Session"),
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/sessions",
            named: "sessions.end",
            about: "Signs them out. The token stops working immediately.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::Nothing,
            refuses: &[],
            changes: true,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/people",
            named: "people.list",
            about: "Who has an account here.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("PersonPage"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/people",
            named: "people.invite",
            about: "Invites somebody, and sends them a link to choose a password with.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("Invitation"),
            answers: Answers::Made("Person"),
            refuses: &[Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/passwords",
            named: "passwords.choose",
            about: "Chooses a password, using a link somebody was sent.",
            who: Who::Anybody,
            parameters: Vec::new(),
            takes: Some("ChosenPassword"),
            answers: Answers::Nothing,
            // The link has been used, or has run out. Not `NotFound`: whether
            // a token exists is not something to answer.
            refuses: &[Code::Invalid],
            changes: true,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/addresses",
            named: "addresses.prove",
            about: "Proves an address, using a link sent to it. Touches nothing else.",
            who: Who::Anybody,
            parameters: Vec::new(),
            takes: Some("Proof"),
            answers: Answers::Nothing,
            refuses: &[Code::Invalid],
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
    fn signing_in_and_proving_an_address_are_different_doors() {
        // The hole that was found in the crate this replaces: one endpoint
        // redeemed a ticket without asking what the ticket was for, so a link
        // sent to prove an address could be used to set a password — and an
        // account that could edit somebody's address could take it over.
        //
        // Here they are two endpoints and two purposes, and the purpose is in
        // the query rather than in a branch after it.
        let named: Vec<&str> = endpoints().iter().map(|e| e.named).collect();

        assert!(named.contains(&"passwords.choose"));
        assert!(named.contains(&"addresses.prove"));
    }

    #[test]
    fn a_capability_is_one_of_the_ones_there_are() {
        assert!(is_a_capability("content"));
        assert!(!is_a_capability("contnet"));
        assert!(!is_a_capability(""));
    }

    #[test]
    fn no_capability_is_named_twice() {
        let mut all = CAPABILITIES.to_vec();
        let count = all.len();
        all.sort_unstable();
        all.dedup();

        assert_eq!(all.len(), count, "a capability is in the list twice");
    }
}
