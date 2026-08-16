//! Where something answers.
//!
//! A slug is what goes in an address after the site's own name, and three
//! things a site does need one: a writing, a term, a form. It is here rather
//! than in whichever of them was written first, because the rule it holds —
//! what an address may be made of — is the same rule in all three and a
//! second copy of it is a second answer to the same question.

use crate::error::{Error, Result};
use crate::say::Say;
use serde::{Deserialize, Serialize};

pub const AN_ADDRESS_IS_LOWERCASE_AND_SHORT: &str = "an_address_is_lowercase_and_short";

/// Where something answers, within its language.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Slug(String);

impl Slug {
    /// Lowercase, digits and dashes, and not beginning or ending with one.
    ///
    /// Checked here rather than in the database alone, so a refusal is a
    /// sentence somebody can read instead of a constraint's name.
    pub fn parse(text: &str) -> Result<Self> {
        let right = !text.is_empty()
            && text.len() <= 128
            && !text.starts_with('-')
            && !text.ends_with('-')
            && text
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');

        if !right {
            return Err(Error::invalid(Say::of(AN_ADDRESS_IS_LOWERCASE_AND_SHORT)));
        }

        Ok(Self(text.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Slug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_what_it_says_it_is() {
        assert!(Slug::parse("hello-there").is_ok());
        assert!(Slug::parse("2026-in-review").is_ok());

        for wrong in [
            "",
            "-leading",
            "trailing-",
            "Upper",
            "with space",
            "e\u{15f}ya",
        ] {
            assert!(Slug::parse(wrong).is_err(), "{wrong} was taken");
        }
    }

    #[test]
    fn an_address_does_not_carry_a_path() {
        // The one that matters: a slug reaching a filesystem or a URL it was
        // not meant to. Neither is this crate's business, which is why the
        // rule is here rather than at whichever of them is nearer the danger.
        for wrong in ["../etc", "a/b", "a.b", "%2e%2e"] {
            assert!(Slug::parse(wrong).is_err(), "{wrong} was taken");
        }
    }
}
