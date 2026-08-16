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

pub mod assistant;
pub mod building;
pub mod mounted;
pub mod overview;
pub mod showing;

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
    all.extend(mavi_health::endpoints());
    all.extend(mavi_analytics::endpoints());
    all.extend(mavi_portable::endpoints());
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

    // The two this crate owns rather than a domain, and each because no domain
    // could: one asks across all of them, the other is a way in to all of them.
    all.push(crate::overview::endpoint());
    all.push(crate::assistant::endpoint());

    all
}

/// Every body those endpoints name.
///
/// Beside the endpoints for the same reason they are collected here at all:
/// one shape is named by several of them, and whether every name resolves is a
/// question no single domain can ask.
#[must_use]
pub fn shapes() -> Vec<mavi_api::Shape> {
    let mut all = Vec::new();

    all.extend(mavi_content::described::shapes());
    all.extend(mavi_taxonomy::described::shapes());
    all.extend(mavi_settings::described::shapes());
    all.extend(mavi_health::described::shapes());
    all.extend(mavi_analytics::described::shapes());
    all.extend(crate::overview::shapes());
    all.extend(mavi_portable::described::shapes());
    all.extend(mavi_media::described::shapes());
    all.extend(mavi_forms::described::shapes());
    all.extend(mavi_people::described::shapes());
    all.extend(mavi_shop::described::shapes());
    all.extend(mavi_courses::described::shapes());
    all.extend(mavi_mail::described::shapes());
    all.extend(mavi_flows::described::shapes());
    all.extend(mavi_design::described::shapes());
    all.extend(mavi_boards::described::shapes());
    all.extend(mavi_audit::described::shapes());
    all.extend(crate::assistant::shapes());

    all
}

/// The whole thing, ready to be asked questions.
#[must_use]
pub fn api() -> Api {
    Api::of(endpoints()).and(shapes())
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
    fn every_body_an_endpoint_names_is_described() {
        // A description whose references point at nothing is one no client can
        // be generated from. This was a hundred and one names once, written
        // out so that the list could only shrink; it is empty, and an endpoint
        // written naming a body nobody described fails here.
        let missing = api().undescribed();

        assert!(missing.is_empty(), "{missing:#?}");
    }

    #[test]
    fn every_reference_in_the_document_resolves() {
        // The other half, and the one that catches a reference written wrongly
        // rather than a body left out: whatever the document says `$ref` to has
        // to be in the document.
        let described = described("0.0.0");
        let schemas = described["components"]["schemas"]
            .as_object()
            .expect("schemas");

        let mut refs = Vec::new();
        collect(&described, &mut refs);

        assert!(!refs.is_empty(), "a description with no references at all");

        for named in refs {
            let Some(named) = named.strip_prefix("#/components/schemas/") else {
                panic!("{named} is not a reference into this document");
            };

            assert!(
                schemas.contains_key(named),
                "{named} is referred to and nothing describes it"
            );
        }
    }

    #[test]
    fn nothing_is_described_that_nothing_refers_to() {
        // The direction nobody thinks of. A shape nothing points at is a type
        // in every generated client that no method ever returns, and it stays
        // there being maintained for as long as somebody assumes it matters.
        let api = api();

        let mut reachable: BTreeSet<&str> = api
            .endpoints
            .iter()
            .flat_map(|endpoint| endpoint.takes.into_iter().chain(endpoint.answers.body()))
            .collect();

        // Whatever those reach, and whatever those reach, until nothing new.
        loop {
            let more: BTreeSet<&str> = api
                .shapes
                .iter()
                .filter(|shape| reachable.contains(shape.named))
                .flat_map(mavi_api::Shape::refers_to)
                .filter(|named| !reachable.contains(named))
                .collect();

            if more.is_empty() {
                break;
            }

            reachable.extend(more);
        }

        let orphaned: Vec<&str> = api
            .shapes
            .iter()
            .map(|shape| shape.named)
            .filter(|named| !reachable.contains(named))
            .collect();

        assert!(orphaned.is_empty(), "{orphaned:#?}");
    }

    /// Every `$ref` anywhere in a document, however deep.
    fn collect(what: &serde_json::Value, into: &mut Vec<String>) {
        match what {
            serde_json::Value::Object(object) => {
                for (key, value) in object {
                    if key == "$ref"
                        && let Some(named) = value.as_str()
                    {
                        into.push(named.to_owned());
                    }

                    collect(value, into);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    collect(item, into);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn what_is_described_is_described_once() {
        let mut named: Vec<&str> = shapes().iter().map(|shape| shape.named).collect();
        let count = named.len();

        named.sort_unstable();
        named.dedup();

        assert_eq!(named.len(), count, "two shapes answer to one name");
    }

    #[test]
    fn no_two_endpoints_anywhere_are_one_tool() {
        // A tool name is an endpoint's with its dots and dashes made
        // underscores, so `writings.throw-away` and a future `writings.throw`
        // under an `away` would be one name — and one of the two would be
        // unreachable to an assistant with nothing saying so.
        let mut by_tool: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();

        for endpoint in endpoints() {
            by_tool
                .entry(mavi_assistant::named(&endpoint))
                .or_default()
                .push(endpoint.named);
        }

        let clashes: Vec<_> = by_tool.values().filter(|named| named.len() > 1).collect();

        assert!(clashes.is_empty(), "{clashes:#?}");
    }

    #[test]
    fn every_tool_is_named_the_way_the_protocol_allows() {
        // Letters, digits, underscores and dashes, and not longer than
        // sixty-four. A name outside that is a tool a client refuses to
        // register, which looks like the tool not existing.
        for endpoint in endpoints() {
            let called = mavi_assistant::named(&endpoint);

            assert!(
                !called.is_empty() && called.len() <= mavi_assistant::called::AT_MOST,
                "{called} is {} characters",
                called.len()
            );
            assert!(
                called
                    .chars()
                    .all(|letter| letter.is_ascii_alphanumeric() || letter == '_' || letter == '-'),
                "{called} has something in it a client will not take"
            );
        }
    }

    #[tokio::test]
    async fn the_refusal_a_client_is_generated_from_is_the_refusal_that_comes_back() {
        // The description and the answer are written in two crates, and only
        // one crate depends on both. This description said a refusal was
        // `error.code` and `error.message` for as long as it did because
        // nothing ever put the two side by side — and a client generated from
        // it would have branched on a field no answer has ever carried.
        let described = described("0.0.0");
        let described = described["components"]["schemas"]["Refusal"]["properties"]
            .as_object()
            .expect("a described refusal");

        let sent = mavi_serve::refusal::answer(&mavi_core::error::Error::invalid(
            mavi_core::say::Say::of("that_form_wants_that_field").with("field", &"email"),
        ));
        let sent = axum::body::to_bytes(sent.into_body(), 64 * 1024)
            .await
            .expect("a body");
        let sent: serde_json::Value = serde_json::from_slice(&sent).expect("a refusal");
        let sent = sent.as_object().expect("an object");

        assert_eq!(
            described.keys().collect::<BTreeSet<_>>(),
            sent.keys().collect::<BTreeSet<_>>(),
        );
    }

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

    /// Open, and about nothing.
    ///
    /// Neither a way in nor something a site shows the world: what asks this
    /// is whatever keeps the process up, many times a minute, and what it is
    /// told is that the process is up. Its own list rather than one of the two
    /// above, because the reason it may be open is its own — **it answers
    /// nothing about the installation**, and the day one of these does answer
    /// something it stops belonging here.
    const SAYS_NOTHING: &[&str] = &["/api/alive"];

    #[test]
    fn everything_anybody_at_all_can_reach_is_open_for_one_of_three_reasons() {
        // "What can somebody who is not signed in get to" answered by reading
        // a list of paths, across the whole installation rather than one
        // domain at a time. Three reasons, each written down: it is what a
        // site shows the world, it is how somebody stops being a stranger, or
        // it says nothing at all.
        let reachable: Vec<&str> = endpoints()
            .iter()
            .filter(|e| e.who == Who::Anybody)
            .map(|e| e.path)
            .filter(|path| {
                !path.starts_with("/api/open/")
                    && !THE_WAYS_IN.contains(path)
                    && !SAYS_NOTHING.contains(path)
            })
            .collect();

        assert!(reachable.is_empty(), "{reachable:#?}");
    }

    #[test]
    fn nothing_that_is_not_open_asks_to_be_treated_as_though_it_were() {
        // The other direction, and the one that matters more: a path in either
        // list above that is no longer open to anybody is a line nobody
        // notices has stopped meaning anything.
        for path in THE_WAYS_IN.iter().chain(SAYS_NOTHING) {
            assert!(
                endpoints()
                    .iter()
                    .any(|e| e.path == *path && e.who == Who::Anybody),
                "{path} is listed as open and nothing open answers there"
            );
        }
    }

    #[test]
    fn what_says_nothing_says_nothing() {
        // The claim the list above is making, held to. An endpoint that is
        // open because it answers nothing about the installation, and then
        // grows a body describing one, is the leak this exists to stop.
        for path in SAYS_NOTHING {
            let endpoint = endpoints()
                .into_iter()
                .find(|e| e.path == *path)
                .expect("something open");

            let shape = endpoint
                .answers
                .body()
                .and_then(|named| shapes().into_iter().find(|shape| shape.named == named));

            let fields: Vec<&str> = shape
                .as_ref()
                .map(|shape| shape.fields().iter().map(|field| field.name).collect())
                .unwrap_or_default();

            assert!(
                fields.len() <= 1,
                "{path} answers with {fields:?}, which is a description of an \
                 installation handed to whoever asks"
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
            mavi_portable::PORTABLE,
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
