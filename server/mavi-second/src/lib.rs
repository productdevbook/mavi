//! The second thing somebody has, besides the password they know.
//!
//! Six digits from an authenticator app, and ten codes on a piece of paper for
//! when the phone is gone.
//!
//! ## Signing in becomes two answers
//!
//! Before this, a password was answered with a session. Now an account with a
//! confirmed second step is answered with **a moment to finish**, and the
//! session comes from finishing it. That is the whole shape of the change, and
//! everything else here is around it.
//!
//! The moment to finish is short-lived and says nothing about who it is for.
//! Somebody holding one has already given a right password, so it is not
//! nothing — but it is not a way in either, and it stops being anything at all
//! within minutes.
//!
//! ## An installation without a sealing key has none of this
//!
//! The secret has to be readable back, so it is sealed rather than hashed, and
//! the key is the host's. An installation that was handed none refuses to set
//! a second step up — plainly, where somebody is asking for one, rather than
//! sealing it with a key baked into the source, which is the appearance of the
//! thing without the thing.

pub mod described;
pub mod digits;
pub mod store;

use mavi_api::{Answers, Endpoint, Method, Who};
use mavi_core::error::Code;

pub use store::{Standing, ToSetUp, WaysBackIn};

/// How long somebody has to finish signing in.
///
/// Five minutes: long enough to find a phone that has gone flat and is on a
/// charger, short enough that a moment left on a shared machine is not a way
/// in an hour later.
pub const HOW_LONG_TO_FINISH: i64 = 5 * 60;

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/second",
            named: "second.standing",
            about: "Whether whoever is asking has a second step, and how many \
                    ways back in are left.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::With("SecondStanding"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/second",
            named: "second.set-up",
            about: "Starts one, and hands back what an app reads. Nothing is \
                    asked of it until the digits have been shown to work.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::Made("SecondToSetUp"),
            // Already confirmed, or an installation with no sealing key.
            refuses: &[Code::Conflict, Code::Internal],
            changes: true,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/second/confirm",
            named: "second.confirm",
            about: "Shows the digits work, and hands over the ways back in. \
                    They are shown once.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("SomeDigits"),
            answers: Answers::With("WaysBackIn"),
            refuses: &[Code::Invalid],
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/second",
            named: "second.take-off",
            about: "Takes it off, and the ways back in with it. Needs the \
                    digits, because taking it off is the thing somebody who \
                    stole a session would do first.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("SomeDigits"),
            answers: Answers::Nothing,
            refuses: &[Code::Invalid],
            changes: true,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/sessions/finish",
            named: "sessions.finish",
            about: "Finishes signing in, with the digits or one of the ways \
                    back in. Answers with the session.",
            who: Who::Anybody,
            parameters: Vec::new(),
            takes: Some("Finishing"),
            // Never `NotFound`: a moment that has run out and one that was
            // never minted answer the same way, or the refusal is a way to ask
            // which ones exist.
            refuses: &[Code::Forbidden],
            answers: Answers::Made("Session"),
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
    fn taking_it_off_asks_for_the_digits() {
        // The thing somebody who stole a session would do first. Every other
        // door here is reached by being signed in; this one is reached by
        // holding the phone as well.
        let off = endpoints()
            .into_iter()
            .find(|endpoint| endpoint.named == "second.take-off")
            .expect("a way off");

        assert_eq!(off.takes, Some("SomeDigits"));
    }

    #[test]
    fn finishing_is_open_because_nobody_is_signed_in_yet() {
        let finish = endpoints()
            .into_iter()
            .find(|endpoint| endpoint.named == "sessions.finish")
            .expect("a way to finish");

        assert_eq!(finish.who, Who::Anybody);
        // Whoever is finishing has already given a right password. What they
        // hold is a moment, and the moment is what the endpoint checks.
        assert_eq!(finish.takes, Some("Finishing"));
    }
}
