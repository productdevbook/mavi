//! Something the site had to say to a list.
//!
//! There is one thing a letter to a list must carry and a letter answering
//! something must not, and it is the way out. So it is not a flag on a
//! sending: a [`Sending`] cannot be made without one, and a letter pressed
//! from one always has it.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use serde::{Deserialize, Serialize};

use crate::letter::Pressed;

pub const A_LETTER_TO_A_LIST_SAYS_HOW_TO_LEAVE_IT: &str = "a_letter_to_a_list_says_how_to_leave_it";
pub const A_SUBJECT_IS_BETWEEN_ONE_AND_THREE_HUNDRED: &str =
    "a_subject_is_between_one_and_three_hundred";
pub const A_LETTER_IS_BETWEEN_ONE_AND_TWENTY_THOUSAND: &str =
    "a_letter_is_between_one_and_twenty_thousand";

/// What the way out is called in a body.
pub const THE_WAY_OUT: &str = "{unsubscribe}";

/// One thing to say to a list, checked.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sending {
    pub subject: String,
    pub body: String,
}

impl Sending {
    /// Refuses a letter to a list that does not say how to leave it.
    ///
    /// Refused rather than quietly appended. Appending is a line the site did
    /// not write, in whatever language this machine happens to speak, at the
    /// bottom of something somebody laid out — and it teaches whoever writes
    /// the next one that leaving it out is fine.
    pub fn checked(subject: &str, body: &str) -> Result<Self> {
        if !(1..=300).contains(&subject.trim().chars().count()) {
            return Err(Error::invalid(Say::of(
                A_SUBJECT_IS_BETWEEN_ONE_AND_THREE_HUNDRED,
            )));
        }

        if !(1..=20_000).contains(&body.trim().chars().count()) {
            return Err(Error::invalid(Say::of(
                A_LETTER_IS_BETWEEN_ONE_AND_TWENTY_THOUSAND,
            )));
        }

        if !body.contains(THE_WAY_OUT) {
            return Err(Error::invalid(
                Say::of(A_LETTER_TO_A_LIST_SAYS_HOW_TO_LEAVE_IT).with("named", &THE_WAY_OUT),
            ));
        }

        Ok(Self {
            subject: subject.to_owned(),
            body: body.to_owned(),
        })
    }

    /// This sending, for one reader.
    ///
    /// The link is theirs alone — it is what says which subscriber pressed it,
    /// and one link for everybody is a button that takes the whole list off.
    #[must_use]
    pub fn to(&self, way_out: &str) -> Pressed {
        Pressed {
            subject: self.subject.clone(),
            body: self.body.replace(THE_WAY_OUT, way_out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_letter_to_a_list_says_how_to_leave_it() {
        let refused = Sending::checked("This month", "Here is the news.").expect_err("a refusal");

        assert_eq!(
            refused.said().expect("a sentence").key,
            A_LETTER_TO_A_LIST_SAYS_HOW_TO_LEAVE_IT
        );
    }

    #[test]
    fn what_goes_out_carries_the_readers_own_way_out() {
        // Their own, not the list's: one link for everybody is a button that
        // takes every reader off at once, pressed by whichever of them is
        // curious.
        let sending = Sending::checked("This month", "Here is the news.\n\n{unsubscribe}")
            .expect("a sending");

        let theirs = sending.to("https://example.test/out/a-token-of-their-own");

        assert!(theirs.body.contains("a-token-of-their-own"));
        assert!(!theirs.body.contains(THE_WAY_OUT), "{}", theirs.body);
    }
}
