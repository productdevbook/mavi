//! What a tool is called, and which endpoint that is.
//!
//! The protocol's names are letters, digits, underscores and dashes. An
//! endpoint's name has a dot in it, and some have a dash already — so the
//! mapping is not something to undo by guessing, and both directions are
//! worked out by walking the same list.

use mavi_api::Endpoint;

/// The longest a tool's name may be, by the protocol.
pub const AT_MOST: usize = 64;

/// One tool: an endpoint, said the way an assistant expects.
#[derive(Clone, Debug)]
pub struct Tool<'a> {
    /// `writings_throw_away`.
    pub called: String,
    /// The endpoint it is. What actually answers, behind the same guard as
    /// every other way in.
    pub endpoint: &'a Endpoint,
}

/// What an endpoint is called when an assistant asks for it.
///
/// A dot separates what a thing is from what is being done to it, and both are
/// part of the name — `writings.read` and `people.read` are two tools, not one
/// called `read`.
#[must_use]
pub fn named(endpoint: &Endpoint) -> String {
    endpoint.named.replace(['.', '-'], "_")
}

/// Every endpoint, as a tool.
///
/// The whole description rather than a chosen few. What an assistant may
/// actually use is decided later and by the same guard as everything else —
/// choosing here would be a second place where that is decided, and the second
/// place is the one that is wrong.
#[must_use]
pub fn tools(endpoints: &[Endpoint]) -> Vec<Tool<'_>> {
    endpoints
        .iter()
        .map(|endpoint| Tool {
            called: named(endpoint),
            endpoint,
        })
        .collect()
}

/// Which endpoint an assistant is asking for, if any.
#[must_use]
pub fn asked_for<'a>(endpoints: &'a [Endpoint], called: &str) -> Option<&'a Endpoint> {
    endpoints.iter().find(|endpoint| named(endpoint) == called)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_api::{Answers, Method, Who};

    fn an_endpoint(named: &'static str) -> Endpoint {
        Endpoint {
            method: Method::Get,
            path: "/api/things",
            named,
            about: "Something.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::With("Thing"),
            refuses: &[],
            changes: false,
        }
    }

    #[test]
    fn a_dot_and_a_dash_are_both_underscores() {
        assert_eq!(
            named(&an_endpoint("writings.throw-away")),
            "writings_throw_away"
        );
        assert_eq!(named(&an_endpoint("writings.list")), "writings_list");
    }

    #[test]
    fn what_a_name_maps_to_is_found_rather_than_taken_apart() {
        // `writings.throw-away` and `writings.throw.away` are one tool name,
        // so no rule turns a tool name back into an endpoint name. The list is
        // walked instead, and this is why.
        let all = [
            an_endpoint("writings.throw-away"),
            an_endpoint("things.read"),
        ];

        assert_eq!(
            asked_for(&all, "writings_throw_away").map(|it| it.named),
            Some("writings.throw-away")
        );
        assert!(asked_for(&all, "nothing_like_this").is_none());
    }
}
