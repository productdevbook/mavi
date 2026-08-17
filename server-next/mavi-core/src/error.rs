use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, MaviError>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Validation,
    Unauthenticated,
    Forbidden,
    NotFound,
    Conflict,
    RateLimited,
    Internal,
}

#[derive(Debug, Error)]
pub enum MaviError {
    #[error("validation failed")]
    Validation { code: String, field: Option<String> },
    #[error("authentication required")]
    Unauthenticated,
    #[error("operation forbidden")]
    Forbidden,
    #[error("resource not found")]
    NotFound { resource: &'static str },
    #[error("operation conflicts with current state")]
    Conflict { code: String },
    #[error("request rate limited")]
    RateLimited,
    #[error("internal error")]
    Internal,
}

impl MaviError {
    #[must_use]
    pub fn validation(code: impl Into<String>) -> Self {
        Self::Validation {
            code: code.into(),
            field: None,
        }
    }

    #[must_use]
    pub fn validation_field(code: impl Into<String>, field: impl Into<String>) -> Self {
        Self::Validation {
            code: code.into(),
            field: Some(field.into()),
        }
    }

    #[must_use]
    pub fn conflict(code: impl Into<String>) -> Self {
        Self::Conflict { code: code.into() }
    }

    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Validation { .. } => ErrorCode::Validation,
            Self::Unauthenticated => ErrorCode::Unauthenticated,
            Self::Forbidden => ErrorCode::Forbidden,
            Self::NotFound { .. } => ErrorCode::NotFound,
            Self::Conflict { .. } => ErrorCode::Conflict,
            Self::RateLimited => ErrorCode::RateLimited,
            Self::Internal => ErrorCode::Internal,
        }
    }
}
