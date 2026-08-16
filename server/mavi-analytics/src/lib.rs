//! How many times the site was read, and how it felt.
//!
//! **Nothing here has ever held an address.** Not hashed, not salted, not for
//! a moment — there is no field for one and no argument that takes one, which
//! is a stronger thing to be able to say than any amount of care taken with
//! one.
//!
//! That costs something and it is worth naming: this counts *views* and not
//! *people*. Telling those apart means knowing where a request came from, and
//! this software is not told that — a request arrives having crossed whatever
//! the host put in front of it, and every arrangement for recovering the
//! original address is a decision about which proxy to believe. A count of
//! people that is wrong, with everybody behind one proxy counted as one, is
//! worse than no count of people at all.
//!
//! The crate this replaces did count people, with a day's salt and a sweep. It
//! was careful and it worked. This is less, and it is the half that cannot go
//! wrong.

pub mod described;
pub mod store;

use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who};
use mavi_core::error::Code;
use mavi_core::grant::{Access, Needs};

pub use store::{Felt, Read};

/// What a browser measures, and the only values a beacon may carry.
///
/// The web's own names rather than this software's: `lcp` is when the biggest
/// thing appeared, `inp` is how long the page took to answer a tap, `cls` is
/// how much moved under somebody's finger, `ttfb` is how long the server took.
pub const WHAT_A_BROWSER_MEASURES: &[&str] = &["lcp", "inp", "cls", "ttfb"];

/// The most days a screen may ask about at once.
///
/// Ninety, because the question a site's owner asks is "how is it going", and
/// a screen that asks for five years is one query that reads the whole table.
pub const AT_MOST_DAYS: i32 = 90;

/// Reading this asks for what the settings ask for: it is about the site
/// rather than about anything in it.
#[must_use]
pub fn to_read() -> Needs {
    Needs::new("settings", Access::View)
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Post,
            path: "/api/open/read",
            named: "open.read",
            about: "Says a page was read. Sent by a reader's own browser, so \
                    everything about it is checked on this side — and what is \
                    counted is that it was read, not by whom.",
            who: Who::Anybody,
            parameters: Vec::new(),
            takes: Some("SomethingRead"),
            // Nothing. A beacon that argues is a beacon that loses the count
            // it was sent for, and there is nothing a browser would do about a
            // refusal anyway.
            answers: Answers::Nothing,
            refuses: &[],
            changes: true,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/analytics",
            named: "analytics.read",
            about: "How many times each page was read.",
            who: Who::AnAccount,
            parameters: vec![Parameter::query(
                "days",
                Is::Number,
                "How many days back, at most ninety.",
            )],
            takes: None,
            answers: Answers::With("ReadList"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/analytics/felt",
            named: "analytics.felt",
            about: "How the site felt to read: the middle, and the bad end.",
            who: Who::AnAccount,
            parameters: vec![Parameter::query(
                "days",
                Is::Number,
                "How many days back, at most ninety.",
            )],
            takes: None,
            answers: Answers::With("FeltList"),
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
    fn what_a_reader_sends_reaches_nothing_that_reads_the_site() {
        // The beacon is the one endpoint here anybody may reach, and it is
        // open under `/api/open/` like everything else a site shows the world.
        // What matters is that it only writes: a reader's browser is not
        // something to answer questions about the site to.
        let beacon = endpoints()
            .into_iter()
            .find(|endpoint| endpoint.named == "open.read")
            .expect("a beacon");

        assert_eq!(beacon.who, Who::Anybody);
        assert_eq!(beacon.answers.body(), None);

        for endpoint in endpoints().iter().filter(|e| e.named != "open.read") {
            assert_eq!(
                endpoint.who,
                Who::AnAccount,
                "{} is not something to tell a stranger",
                endpoint.named
            );
        }
    }
}
