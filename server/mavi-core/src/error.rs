//! One error, one shape, one code a client may branch on.
//!
//! Two things this is built to prevent, both measured in the crate this
//! replaces:
//!
//! An API answering in **two** shapes — its own error object where a handler
//! refused, and a framework's plain-text rejection where an extractor did.
//! A client that parses one throws on the other, and a test suite that has
//! given up asserts `422 || 400` and never reads the body.
//!
//! And a description that names **no** failure at all: every operation
//! declaring one response, `200`, while inheriting four more from the guard
//! above it. Whoever writes a client then discovers the failures in
//! production.
//!
//! So: a failure is this type, it carries a [`Code`], and the code is part of
//! what an endpoint declares rather than something a reader infers.

use std::fmt;

use serde::Serialize;

use crate::say::Say;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// What a client branches on. Stable: a name here is part of the API, and
/// changing one is changing the API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    /// The request was not understood — a shape, a field, a value.
    Invalid,
    /// Nobody is signed in, and this needs somebody.
    Unauthenticated,
    /// Somebody is signed in and may not do this.
    Forbidden,
    /// There is no such thing.
    NotFound,
    /// The request was understood and the state it arrived into refuses it.
    Conflict,
    /// Too much, too fast.
    TooMany,
    /// This end. Nothing the caller can do differently.
    Internal,
}

impl Code {
    /// The HTTP status this answers with. Here rather than in the layer that
    /// serves HTTP, so that a code cannot pick up two different statuses in
    /// two different places.
    #[must_use]
    pub const fn status(self) -> u16 {
        match self {
            Code::Invalid => 422,
            Code::Unauthenticated => 401,
            Code::Forbidden => 403,
            Code::NotFound => 404,
            Code::Conflict => 409,
            Code::TooMany => 429,
            Code::Internal => 500,
        }
    }
}

/// A failure, and what to tell whoever caused it.
///
/// `said` is `None` only for [`Code::Internal`]: a caller cannot act on the
/// inside of this process and telling them about it is a leak rather than a
/// courtesy. Every other code carries a [`Say`], because every other code is
/// something somebody can do differently.
#[derive(Debug)]
pub struct Error {
    code: Code,
    said: Option<Say>,
    /// What actually went wrong, for the log. Never serialised: a database's
    /// complaint names columns, and a caller is not owed the schema.
    cause: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl Error {
    #[must_use]
    pub fn new(code: Code, said: Say) -> Self {
        Self {
            code,
            said: Some(said),
            cause: None,
        }
    }

    #[must_use]
    pub fn invalid(said: Say) -> Self {
        Self::new(Code::Invalid, said)
    }

    #[must_use]
    pub fn forbidden(said: Say) -> Self {
        Self::new(Code::Forbidden, said)
    }

    #[must_use]
    pub fn not_found(said: Say) -> Self {
        Self::new(Code::NotFound, said)
    }

    #[must_use]
    pub fn conflict(said: Say) -> Self {
        Self::new(Code::Conflict, said)
    }

    /// Something this end. The caller is told nothing beyond that it was not
    /// them; the cause is kept for whoever reads the log.
    #[must_use]
    pub fn internal(cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self {
            code: Code::Internal,
            said: None,
            cause: Some(Box::new(cause)),
        }
    }

    #[must_use]
    pub const fn code(&self) -> Code {
        self.code
    }

    #[must_use]
    pub const fn said(&self) -> Option<&Say> {
        self.said.as_ref()
    }

    /// What goes in the log and never in a response.
    #[must_use]
    pub fn cause(&self) -> Option<&(dyn std::error::Error + Send + Sync)> {
        self.cause.as_deref()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.said {
            Some(said) => write!(f, "{}", said.in_english()),
            None => f.write_str("something went wrong"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause
            .as_ref()
            .map(|cause| cause.as_ref() as &(dyn std::error::Error + 'static))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_a_caller_cannot_act_on_reaches_them() {
        let inside = Error::internal(std::io::Error::other("a table nobody should hear about"));

        assert!(
            inside.said().is_none(),
            "an internal error carried a sentence"
        );
        assert_eq!(inside.to_string(), "something went wrong");
        assert!(inside.cause().is_some(), "the log lost why");
    }

    #[test]
    fn a_code_answers_with_one_status_wherever_it_is_read() {
        // Not a tautology: what is being asked is that the mapping lives in
        // one place, so that a second layer serving HTTP cannot answer 403 for
        // a code the first answers 404 for.
        assert_eq!(Code::Forbidden.status(), 403);
        assert_eq!(Code::Conflict.status(), 409);
        assert_eq!(Code::TooMany.status(), 429);
    }
}
