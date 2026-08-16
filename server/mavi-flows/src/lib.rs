//! What a site does by itself.
//!
//! Something happens — an order is paid for, a form comes in — and the site
//! does what somebody arranged it should. A flow is a trigger and a list of
//! steps, and a run is one journey through them.
//!
//! What is decided here rather than at three in the morning: a trigger that
//! nothing emits and a step nothing knows how to do are both refused where the
//! flow is written; a step that has not been told what it needs is refused
//! there too; and **where a step is allowed to call** is a rule of its own, in
//! [`outward`], because that is the one place in this whole family where
//! somebody using the site decides what the server connects to.

pub mod outward;
pub mod run;
pub mod step;
pub mod store;

use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who};
use mavi_core::error::Code;
use mavi_core::grant::{Access, Needs};
use mavi_core::id;
use mavi_core::page::{Key, Keyset, Kind};
use mavi_work::Kind as Work;

pub use outward::to_call;
pub use run::{Went, next_step};
pub use step::{Does, Step, Trigger};

id!(
    /// One flow.
    FlowId
);

id!(
    /// One journey through one.
    RunId
);

pub const FLOWS: &str = "flows";

#[must_use]
pub const fn to_read() -> Needs {
    Needs::new(FLOWS, Access::View)
}

#[must_use]
pub const fn to_write() -> Needs {
    Needs::new(FLOWS, Access::Write)
}

/// Starting whatever was arranged for something that happened.
pub const SOMETHING_HAPPENED: Work = Work::new("flows.start", 5);

/// Running one step of one run.
///
/// Each step is its own piece of work rather than the whole flow being one:
/// a flow that waits a day would otherwise be a worker holding a job for a
/// day, and a worker that dies takes the whole flow with it instead of one
/// step of it.
pub const ONE_STEP: Work = Work::new("flows.step", 5);

pub const BY_RECENT: Keyset = Keyset(&[
    Key::newest("created_at", Kind::Moment),
    Key::newest("id", Kind::Id),
]);

/// A run's own order — when it started, rather than when the row was made.
pub const RUNS_BY_START: Keyset = Keyset(&[
    Key::newest("started_at", Kind::Moment),
    Key::newest("id", Kind::Id),
]);

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    let mut all = the_flows();
    all.extend(the_runs());
    all
}

fn the_flows() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/flows",
            named: "flows.list",
            about: "What this site does by itself.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("FlowPage"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/flows/triggers",
            named: "flows.triggers",
            about: "Everything that can start a flow, and what each one carries.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: None,
            // Answered rather than written in a manual: a panel that has to
            // know the list is a panel that goes out of date on its own.
            answers: Answers::With("TriggerList"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/flows",
            named: "flows.make",
            about: "Arranges one. Every step is checked now rather than when it runs.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("NewFlow"),
            answers: Answers::Made("Flow"),
            refuses: &[Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Patch,
            path: "/api/flows/{id}",
            named: "flows.change",
            about: "Changes what a flow does, or turns it on or off.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which flow.")],
            takes: Some("FlowChanges"),
            answers: Answers::With("Flow"),
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/flows/{id}",
            named: "flows.remove",
            about: "Stops arranging it. Runs that already happened stay.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which flow.")],
            takes: None,
            answers: Answers::Nothing,
            refuses: &[Code::NotFound],
            changes: true,
        },
    ]
}

fn the_runs() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/flows/{id}/runs",
            named: "runs.list",
            about: "What this flow has done, most recent first.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::path("id", Is::Id, "Which flow."),
                Parameter::query("state", Is::Text, "Only runs sitting here."),
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("RunPage"),
            refuses: &[Code::NotFound],
            changes: false,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/runs/{id}",
            named: "runs.read",
            about: "One run: what set it off, and what each step did.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which run.")],
            takes: None,
            answers: Answers::With("Run"),
            refuses: &[Code::NotFound],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/flows/{id}/tries",
            named: "flows.try",
            about: "Runs a flow against something made up, and sends nothing.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which flow.")],
            takes: Some("SomethingMadeUp"),
            // Nothing leaves the machine: no letter, no call. Whoever is
            // arranging a flow should be able to see what it would do without
            // writing to a customer to find out.
            answers: Answers::With("WhatItWouldDo"),
            refuses: &[Code::NotFound],
            // It writes no run and sends nothing, so there is nothing to
            // record. A `POST` because it carries what to try it against.
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
    fn no_two_of_these_are_the_same_route() {
        let clashes = Api::of(endpoints()).clashes();

        assert!(clashes.is_empty(), "{clashes:#?}");
    }

    #[test]
    fn trying_a_flow_sends_nothing_and_so_records_nothing() {
        // The one endpoint here that reads while carrying a body. What counts
        // as a change is what the endpoint says, never the verb it arrived by.
        let trying = endpoints()
            .into_iter()
            .find(|e| e.named == "flows.try")
            .expect("a way to try one");

        assert_eq!(trying.method, Method::Post);
        assert!(!trying.changes);
    }

    #[test]
    fn what_can_start_a_flow_is_answered_rather_than_written_down_somewhere_else() {
        assert!(endpoints().iter().any(|e| e.named == "flows.triggers"));
    }

    #[test]
    fn what_this_domain_asks_for_is_a_capability_the_site_has() {
        assert!(mavi_people::is_a_capability(FLOWS));
    }
}
