//! An address somebody is reached at.
//!
//! What this refuses is deliberately narrow. There is no regular expression
//! here that claims to know which addresses exist, because none does — the
//! only proof an address is real is a letter arriving at it, and that is what
//! proving one is for. What this catches is the shape that cannot work at all:
//! nothing before the `@`, nothing after it, two of them, a space in the
//! middle, a line break somebody pasted in.
//!
//! It is compared folded to lowercase. Not because the local part is
//! case-insensitive — by the letter of the standard it is not — but because
//! every mail host anybody uses treats it so, and an account taken twice at
//! one address by capitalising it is worse than being technically right.

use crate::error::{Error, Result};
use crate::say::Say;
use serde::{Deserialize, Serialize};

pub const THAT_IS_NOT_AN_ADDRESS_A_LETTER_COULD_REACH: &str =
    "that_is_not_an_address_a_letter_could_reach";

/// An address somebody is reached at, folded so two spellings of one address
/// are one address.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Email(String);

impl Email {
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();

        let refuse = || Error::invalid(Say::of(THAT_IS_NOT_AN_ADDRESS_A_LETTER_COULD_REACH));

        if text.len() > 320 || text.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(refuse());
        }

        let mut halves = text.split('@');
        let (Some(local), Some(host), None) = (halves.next(), halves.next(), halves.next()) else {
            return Err(refuse());
        };

        let host_is_reachable = !host.is_empty()
            && host.len() <= 255
            && host.contains('.')
            && !host.starts_with(['.', '-'])
            && !host.ends_with(['.', '-'])
            && !host.contains("..");

        if local.is_empty() || local.len() > 64 || !host_is_reachable {
            return Err(refuse());
        }

        Ok(Self(text.to_lowercase()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Email {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_a_letter_could_reach_is_taken() {
        for right in [
            "someone@example.test",
            "first.last+tag@mail.example.test",
            "a@b.co",
        ] {
            assert!(Email::parse(right).is_ok(), "{right} was refused");
        }
    }

    #[test]
    fn the_shapes_that_cannot_work_at_all_are_refused() {
        for wrong in [
            "",
            "someone",
            "@example.test",
            "someone@",
            "someone@example",
            "two@at@example.test",
            "someone @example.test",
            "someone@.example.test",
            "someone@example.test.",
            "someone@example..test",
        ] {
            assert!(Email::parse(wrong).is_err(), "{wrong:?} was taken");
        }
    }

    #[test]
    fn one_address_spelled_two_ways_is_one_address() {
        // The failure this prevents: two accounts at what the mail host
        // considers one address, and whichever of them a letter reaches is
        // whichever the host decided.
        let plain = Email::parse("someone@example.test").expect("an address");
        let shouted = Email::parse("  SomeOne@Example.Test  ").expect("an address");

        assert_eq!(plain, shouted);
    }

    #[test]
    fn a_line_break_pasted_in_does_not_survive() {
        // A newline in an address is a header somebody else wrote, if it ever
        // reaches a letter. It does not get that far.
        assert!(Email::parse("someone@example.test\nBcc: nobody@example.test").is_err());
    }
}
