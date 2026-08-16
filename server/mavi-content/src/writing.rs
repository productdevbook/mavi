//! One thing a site wrote.

use chrono::{DateTime, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::id;
use mavi_core::say::Say;
pub use mavi_core::slug::{AN_ADDRESS_IS_LOWERCASE_AND_SHORT, Slug};
use serde::{Deserialize, Serialize};

id!(
    /// One writing.
    WritingId
);

pub const A_KIND_IS_LOWERCASE_AND_SHORT: &str = "a_kind_is_lowercase_and_short";
pub const SOMETHING_ELSE_ANSWERS_AT_THAT_ADDRESS: &str = "something_else_answers_at_that_address";
pub const A_TITLE_IS_BETWEEN_ONE_AND_TWO_HUNDRED: &str = "a_title_is_between_one_and_two_hundred";
pub const NOTHING_IS_WRITTEN_AT_THAT_ADDRESS: &str = "nothing_is_written_at_that_address";

/// What kind of thing this is.
///
/// A string rather than an enum, on purpose: `post` and `page` are here from
/// the start and a site adds its own. An enum would mean a migration every
/// time somebody has an idea, and a CMS whose kinds are fixed at compile time
/// is a CMS for one site.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Kind(String);

impl Kind {
    pub fn parse(text: &str) -> Result<Self> {
        let right = !text.is_empty()
            && text.len() <= 31
            && text.starts_with(|c: char| c.is_ascii_lowercase())
            && text
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');

        if !right {
            return Err(Error::invalid(Say::of(A_KIND_IS_LOWERCASE_AND_SHORT)));
        }

        Ok(Self(text.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Whether anybody outside can read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Draft,
    Published,
}

impl State {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            State::Draft => "draft",
            State::Published => "published",
        }
    }
}

/// One writing, as anybody reading it sees it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Writing {
    pub id: WritingId,
    pub kind: Kind,
    pub language: String,
    pub slug: Slug,
    pub title: String,
    pub excerpt: Option<String>,
    pub body: String,
    /// Whatever this kind carries beyond the above.
    pub fields: serde_json::Value,
    pub state: State,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// What writing one asks for.
///
/// Serialised as well as read, so the test beside the description can put a
/// real one through the same serialiser and compare its fields with what the
/// API says it takes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct New {
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub fields: serde_json::Value,
    /// Absent means a draft. A date in the future is a thing that goes out on
    /// it; the scheduler is what publishes it, not this.
    pub publish_at: Option<DateTime<Utc>>,
}

impl New {
    /// Everything checked before anything is written, so a refusal names the
    /// first thing wrong rather than whatever the database happened to reach.
    pub fn checked(&self) -> Result<(Kind, Slug)> {
        let kind = Kind::parse(&self.kind)?;
        let slug = Slug::parse(&self.slug)?;

        let length = self.title.trim().chars().count();
        if !(1..=200).contains(&length) {
            return Err(Error::invalid(Say::of(
                A_TITLE_IS_BETWEEN_ONE_AND_TWO_HUNDRED,
            )));
        }

        Ok((kind, slug))
    }

    /// Whether this is published, and when — the pair the schema checks
    /// together, decided in one place rather than at each caller.
    #[must_use]
    pub fn goes_out(&self) -> (State, Option<DateTime<Utc>>) {
        match self.publish_at {
            Some(at) => (State::Published, Some(at)),
            None => (State::Draft, None),
        }
    }
}

/// Turning what the database said about a unique index into what a person
/// reads.
///
/// The alternative is checking first and writing second, which is two
/// statements and a race between them: two requests both find the address
/// free, and one of them is wrong by the time it writes. Letting the database
/// refuse and reading its refusal is the version with no gap in it.
#[must_use]
pub fn taken(cause: &sqlx::Error) -> bool {
    matches!(cause, sqlx::Error::Database(db) if db.constraint() == Some("writings_address"))
}

/// What to say when it is.
#[must_use]
pub fn address_is_taken() -> Error {
    Error::conflict(Say::of(SOMETHING_ELSE_ANSWERS_AT_THAT_ADDRESS))
}

/// What to say when there is no such thing.
#[must_use]
pub fn nothing_there() -> Error {
    Error::not_found(Say::of(NOTHING_IS_WRITTEN_AT_THAT_ADDRESS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_is_a_name_and_not_a_sentence() {
        for right in ["post", "page", "course", "holiday_let", "kind9"] {
            assert!(Kind::parse(right).is_ok(), "{right} was refused");
        }

        for wrong in [
            "",
            "Post",
            "9lives",
            "with space",
            "with-dash",
            &"x".repeat(32),
        ] {
            assert!(
                Kind::parse(wrong).is_err(),
                "{wrong:?} was taken for a kind"
            );
        }
    }

    #[test]
    fn published_and_when_are_decided_together() {
        // The schema checks that a published row has a date and a draft has
        // none. This is the one place that pairing is made, so no caller can
        // make half of it.
        let draft = New {
            kind: "post".into(),
            language: "en".into(),
            slug: "hello".into(),
            title: "Hello".into(),
            excerpt: None,
            body: String::new(),
            fields: serde_json::Value::Null,
            publish_at: None,
        };

        assert_eq!(draft.goes_out(), (State::Draft, None));

        let at = Utc::now();
        let going = New {
            publish_at: Some(at),
            ..draft
        };

        assert_eq!(going.goes_out(), (State::Published, Some(at)));
    }

    #[test]
    fn a_title_of_spaces_is_not_a_title() {
        let blank = New {
            kind: "post".into(),
            language: "en".into(),
            slug: "hello".into(),
            title: "   ".into(),
            excerpt: None,
            body: String::new(),
            fields: serde_json::Value::Null,
            publish_at: None,
        };

        let refused = blank.checked().expect_err("a title of spaces was taken");
        assert_eq!(
            refused.said().expect("a refusal").key,
            A_TITLE_IS_BETWEEN_ONE_AND_TWO_HUNDRED
        );
    }
}
