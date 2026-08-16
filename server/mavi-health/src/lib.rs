//! Whether this installation is well.
//!
//! Two questions that look like one and are not, which is why they are two
//! endpoints:
//!
//! **Is it alive?** Asked by whatever is keeping the process up — a container
//! runtime, a load balancer — many times a minute, by nobody. So it answers
//! nothing beyond yes or no. A detailed health page open to anybody is a
//! description of somebody's installation handed to whoever asks: how many
//! pages it has, whether its last publish failed, how much work is stuck.
//!
//! **What is wrong with it?** Asked by a person looking at a screen after
//! something has gone wrong, and it needs a grant like anything else.
//!
//! The crate this replaces had only the second, behind a grant, which meant
//! there was nothing a container runtime could ask — and a process that cannot
//! be asked whether it is alive is one that gets restarted on a timer instead.

pub mod described;
pub mod store;

use mavi_api::{Answers, Code, Endpoint, Method, Who};
use mavi_core::grant::{Access, Needs};

pub use store::{Check, Health, look_at};

/// What reading this asks for. The same grant as the settings, because what it
/// answers is about the installation rather than about anything in it.
#[must_use]
pub fn to_read() -> Needs {
    Needs::new("settings", Access::View)
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/alive",
            named: "health.alive",
            about: "Whether this installation can answer at all. For whatever \
                    keeps the process up, and it says nothing else.",
            who: Who::Anybody,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::With("Alive"),
            // Nothing. What a caller does about it is restart the process, and
            // there is no shape of refusal that helps with that — an
            // installation that cannot answer this does not answer.
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/health",
            named: "health.read",
            about: "What is wrong with this installation, where anything is.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::With("Health"),
            refuses: &[Code::Internal],
            changes: false,
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
    fn what_anybody_may_ask_says_nothing_about_the_installation() {
        // The whole reason there are two. A health page open to anybody is a
        // description of somebody's installation — how many pages it has,
        // whether its last publish failed — handed to whoever asks for it.
        let alive = endpoints()
            .into_iter()
            .find(|endpoint| endpoint.named == "health.alive")
            .expect("something to ask");

        assert_eq!(alive.who, Who::Anybody);
        assert_eq!(alive.answers.body(), Some("Alive"));

        let health = endpoints()
            .into_iter()
            .find(|endpoint| endpoint.named == "health.read")
            .expect("the detail");

        assert_eq!(health.who, Who::AnAccount);
    }
}
