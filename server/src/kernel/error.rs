//! What went wrong, and what a caller is told about it.
//!
//! One error type for the whole build, so a handler returns `Result` and the
//! router turns it into an answer. What a person sees is a key rather than a
//! sentence — the panel says it in their own language — and what is written
//! down is the sentence in English, which is what a log is read in.
//!
//! Anything that is not somebody being told no is a five hundred that says
//! nothing: a caller who cannot act on the detail is a caller being handed
//! the shape of the database.
use std::collections::BTreeMap;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use super::say::Say;

pub type Result<T, E = AppError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0} not found")]
    NotFound(&'static str),

    #[error("{0}")]
    Invalid(Say),

    #[error("not signed in")]
    Unauthenticated,

    #[error("not allowed")]
    Forbidden,

    /// Refused, and with something to say about why. The engine's own no is
    /// [`AppError::Forbidden`] and says nothing, because what a policy would
    /// have needed is not a caller's business; this is for the refusals a
    /// handler makes with a reason a person can act on.
    #[error("{0}")]
    Refused(Say),

    #[error("{0}")]
    Conflict(Say),

    /// The password was right and it is not enough. Said apart from every
    /// other no so that the panel can ask for the digits rather than telling
    /// somebody their password is wrong.
    #[error("that account is asked for a second factor")]
    SecondFactorRequired,

    #[error("too many requests")]
    RateLimited,

    #[error("no site answers for that address")]
    UnknownHost,

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error("{0}")]
    Bug(&'static str),
}

/// Machine-readable, from a fixed list; the message beside it is for a person.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    NotFound,
    Invalid,
    Unauthenticated,
    Forbidden,
    Conflict,
    SecondFactorRequired,
    RateLimited,
    UnknownHost,
    Internal,
}

impl AppError {
    #[must_use]
    pub fn code(&self) -> Code {
        match self {
            AppError::NotFound(_) => Code::NotFound,
            AppError::Invalid(_) => Code::Invalid,
            AppError::Unauthenticated => Code::Unauthenticated,
            AppError::Forbidden | AppError::Refused(_) => Code::Forbidden,
            AppError::Conflict(_) => Code::Conflict,
            AppError::SecondFactorRequired => Code::SecondFactorRequired,
            AppError::RateLimited => Code::RateLimited,
            AppError::UnknownHost => Code::UnknownHost,
            AppError::Database(_) | AppError::Bug(_) => Code::Internal,
        }
    }

    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self.code() {
            Code::NotFound | Code::UnknownHost => StatusCode::NOT_FOUND,
            Code::Invalid => StatusCode::UNPROCESSABLE_ENTITY,
            Code::Unauthenticated | Code::SecondFactorRequired => StatusCode::UNAUTHORIZED,
            Code::Forbidden => StatusCode::FORBIDDEN,
            Code::Conflict => StatusCode::CONFLICT,
            Code::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Code::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// What was said, where it was said as a key.
    #[must_use]
    pub fn said(&self) -> Option<&Say> {
        match self {
            AppError::Invalid(say) | AppError::Refused(say) | AppError::Conflict(say) => Some(say),
            _ => None,
        }
    }

    /// Somebody being told no, or something being wrong. Only the second is
    /// worth an alert.
    #[must_use]
    pub fn is_bug(&self) -> bool {
        matches!(self, AppError::Database(_) | AppError::Bug(_))
    }
}

#[derive(Debug, Serialize)]
struct Body {
    error: Shape,
}

#[derive(Debug, Serialize)]
struct Shape {
    code: Code,
    /// What is wrong, as a key the panel looks up in its own language. Null
    /// where the refusal has no key of its own — a rate limit, a bug.
    key: Option<&'static str>,
    /// What the key names, where it names anything.
    named: BTreeMap<&'static str, String>,
    /// The English, for a log and for a client with no catalogue.
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let message = if self.is_bug() {
            tracing::error!(error = %self, code = ?self.code(), "request failed");
            "something went wrong".to_owned()
        } else {
            tracing::warn!(code = ?self.code(), "request refused");
            self.to_string()
        };

        let said = self.said();

        (
            self.status(),
            Json(Body {
                error: Shape {
                    code: self.code(),
                    key: said.map(Say::key),
                    named: said
                        .map(|say| {
                            say.named()
                                .iter()
                                .map(|(what, value)| (*what, value.clone()))
                                .collect()
                        })
                        .unwrap_or_default(),
                    message,
                },
            }),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_database_failure_does_not_reach_the_person_asking() {
        let error = AppError::Database(sqlx::Error::RowNotFound);
        let rendered = format!("{error}");

        assert!(error.is_bug());
        assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(rendered.contains("no rows"), "{rendered}");
    }

    #[test]
    fn being_told_no_is_not_an_alert() {
        for error in [
            AppError::Forbidden,
            AppError::Unauthenticated,
            AppError::RateLimited,
            AppError::NotFound("post"),
            AppError::Invalid(Say::of("no")),
            AppError::SecondFactorRequired,
        ] {
            assert!(!error.is_bug(), "{error}");
        }
    }
}
