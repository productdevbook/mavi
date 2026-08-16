//! What a refusal looks like coming back.
//!
//! One shape for every refusal this installation can make, including the ones
//! nobody wrote a handler for: an unknown path, a method that is not on that
//! path, a body that is not JSON. The description says every operation can
//! answer this shape, and that is only true if everything actually does —
//! including the parts of the router nobody wrote.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use mavi_core::error::{Code, Error};
use mavi_core::say::Say;
use serde::Serialize;

pub const NOTHING_ANSWERS_THERE: &str = "nothing_answers_there";
pub const NOT_THAT_WAY: &str = "not_that_way";
pub const THAT_IS_NOT_SOMETHING_THIS_UNDERSTANDS: &str = "that_is_not_something_this_understands";

/// A refusal, as it goes back.
///
/// `key` is what a client branches on and `named` is what a sentence would
/// have filled in. `said` is the English, so that something with no wording of
/// its own has something to show — never the only thing there, because whoever
/// reads it may not read English.
#[derive(Debug, Serialize)]
pub struct Refusal {
    pub key: &'static str,
    pub named: std::collections::BTreeMap<&'static str, String>,
    pub said: String,
}

impl Refusal {
    #[must_use]
    pub fn of(say: &Say) -> Self {
        Self {
            key: say.key,
            named: say.named.clone(),
            said: say.in_english(),
        }
    }
}

/// Turns an error into an answer.
///
/// The internal one carries nothing. Whatever went wrong is this machine's
/// business and the caller cannot act on it; what they get is the status and a
/// key, and what the operator gets is the error in the log with its cause
/// attached.
#[must_use]
pub fn answer(error: &Error) -> Response {
    let status =
        StatusCode::from_u16(error.code().status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let refusal = error.said().map_or_else(
        || Refusal::of(&Say::of("something_went_wrong_here")),
        Refusal::of,
    );

    (status, Json(refusal)).into_response()
}

/// What the router itself answers where no endpoint does.
#[must_use]
pub fn nothing_answers_there() -> Response {
    answer(&Error::new(Code::NotFound, Say::of(NOTHING_ANSWERS_THERE)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_core::error::Error;

    #[test]
    fn a_refusal_carries_the_key_and_what_it_names() {
        let error = Error::invalid(Say::of("that_form_wants_that_field").with("field", &"email"));
        let refusal = Refusal::of(error.said().expect("a sentence"));

        assert_eq!(refusal.key, "that_form_wants_that_field");
        assert_eq!(
            refusal.named.get("field").map(String::as_str),
            Some("email")
        );
    }

    #[test]
    fn what_went_wrong_inside_does_not_come_back_out() {
        // The caller cannot act on it, and a stack trace in an answer is a
        // description of this machine handed to whoever asked for it.
        let inside = Error::internal(std::io::Error::other(
            "connection to the database at 10.0.0.5 refused",
        ));

        assert!(inside.said().is_none());

        let refusal = inside.said().map_or_else(
            || Refusal::of(&Say::of("something_went_wrong_here")),
            Refusal::of,
        );

        assert!(!refusal.said.contains("10.0.0.5"));
        assert!(!refusal.said.contains("database"));
    }
}
