//! What a site has, in one answer.
//!
//! The one endpoint that reaches across every domain, and it is here for that
//! reason: no crate may ask about another, and this is the crate whose whole
//! job is the questions no one of them can ask.
//!
//! Eleven counts in one query rather than eleven calls, because the screen
//! that shows them is the first one anybody opens — and eleven round trips
//! before a panel draws anything is what a slow panel is made of.

use mavi_api::{Answers, Endpoint, Field, Is, Method, Of, Shape, Who};
use mavi_core::grant::{Access, Needs};

/// Reading it asks for what the settings ask for: it is about the site rather
/// than about anything in it, and every number in it is one somebody could
/// have counted from a listing they already hold.
#[must_use]
pub fn to_read() -> Needs {
    Needs::new("settings", Access::View)
}

#[must_use]
pub fn endpoint() -> Endpoint {
    Endpoint {
        method: Method::Get,
        path: "/api/overview",
        named: "site.overview",
        about: "What this site has, counted. The first screen anybody opens.",
        who: Who::AnAccount,
        parameters: Vec::new(),
        takes: None,
        answers: Answers::With("Overview"),
        refuses: &[],
        changes: false,
    }
}

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![Shape::new(
        "Overview",
        "What this site has. Every number here is one somebody could have \
         counted from a listing they already hold — this is the same answer in \
         one call rather than eleven.",
        vec![
            Field::new(
                "writings",
                Of::One(Is::Number),
                "How many, including drafts.",
            ),
            Field::new(
                "published",
                Of::One(Is::Number),
                "How many of them are out.",
            ),
            Field::new("files", Of::One(Is::Number), "How many have been uploaded."),
            Field::new("bytes", Of::One(Is::Number), "What they come to."),
            Field::new("forms", Of::One(Is::Number), "How many the site asks."),
            Field::new(
                "unread",
                Of::One(Is::Number),
                "What people sent that nobody has read.",
            ),
            Field::new(
                "readers",
                Of::One(Is::Number),
                "How many may still be written to. Not how many rows there \
                 are: a list of nine hundred nobody may write to is a number \
                 that tells whoever reads it the wrong thing.",
            ),
            Field::new(
                "students",
                Of::One(Is::Number),
                "How many are learning here.",
            ),
            Field::new("orders", Of::One(Is::Number), "How many have been placed."),
            Field::new(
                "flows_on",
                Of::One(Is::Number),
                "How many run by themselves.",
            ),
            Field::new(
                "work_given_up_on",
                Of::One(Is::Number),
                "Work the queue has stopped trying. A letter nobody received \
                 or a build nobody got — on the first screen, because nothing \
                 else says so.",
            ),
        ],
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_api::Api;

    #[test]
    fn it_says_everything_about_itself() {
        let holes = Api::of(vec![endpoint()]).holes();

        assert!(holes.is_empty(), "{holes:#?}");
    }

    #[test]
    fn it_answers_nothing_a_listing_would_not() {
        // The rule that keeps this from becoming a second API. Every field is
        // a count; the moment one of them is a name or an address, this is a
        // listing with a different grant on it.
        for field in shapes()[0].fields() {
            assert!(
                matches!(field.of, Of::One(Is::Number)),
                "{} is not a count",
                field.name
            );
        }
    }
}
