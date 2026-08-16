//! Everything this installation answers, in one place.
//!
//! Each domain describes its own endpoints and asks its own questions of them.
//! There are questions no domain can ask about itself, and they are the ones
//! that go wrong quietly:
//!
//! - two crates describing **one route** — the same path with its hole named
//!   differently, which a router may refuse outright when the process starts,
//!   so the failure is the whole server rather than one endpoint;
//! - two crates giving an endpoint **the same name**, which a generated client
//!   turns into one method that calls whichever came second;
//! - a capability that **nothing asks for**, or one asked for that the site
//!   does not have.
//!
//! Nothing here mounts anything. It is the list, and the tests are what make
//! the list worth having.

pub mod mounted;

use mavi_api::{Api, Endpoint};

/// Every endpoint this installation has.
///
/// The order is the order somebody reading the description will see, so it is
/// what a site does rather than what the crates are called: what it writes,
/// what it holds, who works on it, what it sells, what it teaches, what it
/// sends, what it does by itself, how it looks, and what it has done.
#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    let mut all = Vec::new();

    all.extend(mavi_people::endpoints());
    all.extend(mavi_settings::endpoints());
    all.extend(mavi_content::endpoints());
    all.extend(mavi_taxonomy::endpoints());
    all.extend(mavi_media::endpoints());
    all.extend(mavi_forms::endpoints());
    all.extend(mavi_shop::endpoints());
    all.extend(mavi_courses::endpoints());
    all.extend(mavi_mail::endpoints());
    all.extend(mavi_flows::endpoints());
    all.extend(mavi_design::endpoints());
    all.extend(mavi_boards::endpoints());
    all.extend(mavi_audit::endpoints());

    all
}

/// The whole thing, ready to be asked questions.
#[must_use]
pub fn api() -> Api {
    Api::of(endpoints())
}

/// The description a client is generated from.
#[must_use]
pub fn described(version: &str) -> serde_json::Value {
    mavi_api::openapi(&api(), version)
}

/// Every kind of work this installation runs.
///
/// Declared here for the same reason the endpoints are: the queue refuses a
/// kind it does not know, and what it knows is this list. A domain that adds a
/// job kind and does not add it here has a flow that queues work nothing will
/// ever take — which is exactly what the queue's own refusal is for, and it
/// happens the moment somebody tries rather than quietly.
#[must_use]
pub fn work() -> Vec<mavi_work::Kind> {
    vec![
        mavi_shop::PUT_BACK_WHAT_NOBODY_PAID_FOR,
        mavi_flows::SOMETHING_HAPPENED,
        mavi_flows::ONE_STEP,
        mavi_design::BUILD_A_LOOK,
        mavi_design::PUT_IT_LIVE,
    ]
}

/// What happens on its own, and how often.
///
/// Beside the list of what work exists, because a schedule for work nothing
/// runs is a tick that queues something the queue then refuses — and the two
/// lists being in one file is what makes that a thing somebody notices.
#[must_use]
pub fn on_a_timer() -> Vec<mavi_work::Often> {
    vec![
        // Stock held for a checkout nobody paid for. Five minutes because a
        // hold lasts thirty: something that has run out is on the shelf again
        // within a sixth of the time it was held for.
        mavi_work::Often::minutes(mavi_shop::PUT_BACK_WHAT_NOBODY_PAID_FOR.name, 5),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_api::Who;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn every_endpoint_this_installation_has_says_everything_about_itself() {
        // Each domain asks this of its own. Asking it of all of them is what
        // catches a domain that was added and never wired in.
        let holes = api().holes();

        assert!(holes.is_empty(), "{holes:#?}");
    }

    #[test]
    fn no_two_endpoints_anywhere_are_the_same_route() {
        // The question no crate can ask about itself. Two paths of one shape
        // whose hole is named differently is one route, and a router is
        // entitled to refuse the pair when the process starts — so the failure
        // is the whole server rather than one endpoint.
        let clashes = api().clashes();

        assert!(clashes.is_empty(), "{clashes:#?}");
    }

    #[test]
    fn no_two_endpoints_anywhere_are_called_the_same_thing() {
        // A generated client makes one method per name. Two endpoints with one
        // name is one method that calls whichever was written second, and the
        // other endpoint is unreachable from every client at once.
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();

        for endpoint in endpoints() {
            *seen.entry(endpoint.named).or_default() += 1;
        }

        let twice: Vec<&&str> = seen
            .iter()
            .filter(|(_, count)| **count > 1)
            .map(|(named, _)| named)
            .collect();

        assert!(twice.is_empty(), "{twice:#?}");
    }

    /// The ways in.
    ///
    /// Anybody may reach these, and none of them is under `/api/open/`,
    /// because they are not what a site shows the world — they are how
    /// somebody stops being a stranger: signing in, asking for a password
    /// link, proving an address. The list is written out here so that a fourth
    /// one is a line somebody adds on purpose in this file, rather than a
    /// declaration in a domain that nothing ever compares against anything.
    const THE_WAYS_IN: &[&str] = &[
        // The one that exists before anybody does. It answers once, and after
        // that it is a conflict rather than a door.
        "/api/setup",
        "/api/sessions",
        "/api/passwords",
        "/api/addresses",
    ];

    #[test]
    fn everything_anybody_at_all_can_reach_is_a_way_in_or_says_it_is_open() {
        // "What can somebody who is not signed in get to" answered by reading
        // a list of paths, across the whole installation rather than one
        // domain at a time.
        let reachable: Vec<&str> = endpoints()
            .iter()
            .filter(|e| e.who == Who::Anybody)
            .map(|e| e.path)
            .filter(|path| !path.starts_with("/api/open/") && !THE_WAYS_IN.contains(path))
            .collect();

        assert!(reachable.is_empty(), "{reachable:#?}");
    }

    #[test]
    fn nothing_that_is_not_a_way_in_asks_to_be_treated_as_one() {
        // The other direction, and the one that matters more: a path in the
        // list above that is no longer open to anybody is a line nobody
        // notices has stopped meaning anything.
        for way_in in THE_WAYS_IN {
            assert!(
                endpoints()
                    .iter()
                    .any(|e| e.path == *way_in && e.who == Who::Anybody),
                "{way_in} is listed as a way in and nothing open answers there"
            );
        }
    }

    #[test]
    fn every_capability_this_site_has_is_asked_for_by_something() {
        // A capability nothing asks for is a switch in the panel that does
        // nothing, and somebody grants it believing it did something.
        let asked: BTreeSet<&str> = [
            mavi_content::CONTENT,
            mavi_media::MEDIA,
            mavi_taxonomy::TAXONOMY,
            mavi_forms::FORMS,
            mavi_mail::MAIL,
            mavi_flows::FLOWS,
            mavi_courses::COURSES,
            mavi_shop::SHOP,
            mavi_people::PEOPLE,
            mavi_settings::SETTINGS,
            mavi_design::PUBLISH,
            mavi_design::DESIGN,
            mavi_boards::BOARDS,
            mavi_audit::AUDIT,
        ]
        .into_iter()
        .collect();

        let the_site_has: BTreeSet<&str> = mavi_people::CAPABILITIES.iter().copied().collect();

        assert_eq!(
            the_site_has, asked,
            "the capabilities a site can grant and the ones anything asks for are not the same list"
        );
    }

    #[test]
    fn nothing_is_scheduled_that_nothing_runs() {
        // A tick for work the queue refuses is a row that fails every five
        // minutes for ever, and the only sign of it is a dead job nobody
        // reads.
        let runs: BTreeSet<&str> = work().iter().map(|kind| kind.name).collect();

        for often in on_a_timer() {
            assert!(
                runs.contains(often.kind),
                "{} is on a timer and nothing runs it",
                often.kind
            );
        }
    }

    #[test]
    fn every_kind_of_work_is_named_once() {
        let mut names: Vec<&str> = work().iter().map(|kind| kind.name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();

        assert_eq!(names.len(), count, "two kinds of work share a name");
    }

    #[test]
    fn the_queue_this_installation_runs_takes_every_kind_it_declares() {
        let queue = mavi_work::Queue::of(&work());

        for kind in work() {
            assert!(
                queue.runs(kind.name).is_some(),
                "{} is declared and the queue does not run it",
                kind.name
            );
        }
    }

    #[test]
    fn the_description_carries_every_endpoint() {
        // Counted rather than eyeballed. A path with two verbs on it is one
        // entry in the description and two endpoints here, so the count is of
        // operations rather than of paths.
        let described = described("0.0.0");

        let operations: usize = described["paths"]
            .as_object()
            .expect("paths")
            .values()
            .map(|path| path.as_object().map_or(0, serde_json::Map::len))
            .sum();

        assert_eq!(operations, endpoints().len());
    }

    #[test]
    fn how_much_of_a_site_this_is() {
        // Not a rule — a number, printed where somebody reading the tests can
        // see it. The API this replaces described 177 operations across 121
        // paths, and every one of them was missing its parameters, its
        // failures and its way of authenticating.
        let paths: BTreeSet<&str> = endpoints().iter().map(|e| e.path).collect();

        assert!(
            endpoints().len() >= 80,
            "{} operations across {} paths",
            endpoints().len(),
            paths.len()
        );
    }
}
