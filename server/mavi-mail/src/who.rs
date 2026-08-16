//! Who a site may write to, and why it is writing.
//!
//! One rule, and it is the rule that keeps a site's mail arriving at all: what
//! somebody did with the newsletter has nothing to do with whether they can be
//! sent the link that gets them back into their own account.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use serde::{Deserialize, Serialize};

pub const THEY_ASKED_NOT_TO_BE_WRITTEN_TO: &str = "they_asked_not_to_be_written_to";
pub const THAT_ADDRESS_DOES_NOT_TAKE_LETTERS: &str = "that_address_does_not_take_letters";
pub const THEY_SAID_THAT_WAS_SPAM: &str = "they_said_that_was_spam";

/// Why the site is writing.
///
/// Two, and they are two types rather than a column with a default. A default
/// means somebody can send a thousand people a newsletter that says it is a
/// receipt, and the way anybody finds out is that the site stops being
/// delivered anywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// Because they did something: a password link, an invitation, a receipt.
    /// It carries no way out of it, because there is nothing to be out of.
    Because,
    /// Because the site had something to say to a list. Never without a way
    /// out of it, which is what [`crate::sending::Sending`] is for.
    ToAList,
}

/// Where somebody stands with a site's mail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    Subscribed,
    /// They followed the link at the bottom. About the list, and about
    /// nothing else.
    Unsubscribed,
    /// The address gave it back. Nothing this site did.
    Bounced,
    /// They pressed the button that says this was spam.
    Complained,
}

impl Standing {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Standing::Subscribed => "subscribed",
            Standing::Unsubscribed => "unsubscribed",
            Standing::Bounced => "bounced",
            Standing::Complained => "complained",
        }
    }
}

/// Whether this letter may go to somebody standing like this.
///
/// The three refusals are three different things and say so:
///
/// **Unsubscribed** is about the list and only the list. Somebody who left the
/// newsletter and then forgot their password must still get the link — the
/// alternative is an account nobody can get back into because of a decision
/// about something else entirely.
///
/// **Bounced** stops the list and not the rest. A bounce is the address, not
/// the person: a full mailbox is emptied, a typo is corrected, and the letter
/// that lets them fix it is the one being refused here. Sending campaigns to
/// an address that gave the last one back is what gets a whole domain marked
/// as a sender nobody accepts.
///
/// **Complained** stops everything. They pressed the button that says this was
/// spam, and the next letter of any sort is the one that has the site's mail
/// stopped for every other reader as well.
pub fn may_write(standing: Standing, purpose: Purpose) -> Result<()> {
    let refusal = match (standing, purpose) {
        (Standing::Subscribed, _)
        | (Standing::Unsubscribed | Standing::Bounced, Purpose::Because) => {
            return Ok(());
        }
        (Standing::Unsubscribed, Purpose::ToAList) => THEY_ASKED_NOT_TO_BE_WRITTEN_TO,
        (Standing::Bounced, Purpose::ToAList) => THAT_ADDRESS_DOES_NOT_TAKE_LETTERS,
        (Standing::Complained, _) => THEY_SAID_THAT_WAS_SPAM,
    };

    Err(Error::forbidden(Say::of(refusal)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(standing: Standing, purpose: Purpose) -> &'static str {
        may_write(standing, purpose)
            .expect_err("a refusal")
            .said()
            .expect("a sentence")
            .key
    }

    #[test]
    fn leaving_the_newsletter_does_not_lock_somebody_out_of_their_account() {
        // The one that matters. A single "do not write to them" flag, read
        // before every letter, is an account nobody can get back into.
        assert!(may_write(Standing::Unsubscribed, Purpose::Because).is_ok());

        assert_eq!(
            refused(Standing::Unsubscribed, Purpose::ToAList),
            THEY_ASKED_NOT_TO_BE_WRITTEN_TO
        );
    }

    #[test]
    fn an_address_that_gave_a_letter_back_still_gets_the_one_that_fixes_it() {
        assert!(may_write(Standing::Bounced, Purpose::Because).is_ok());

        assert_eq!(
            refused(Standing::Bounced, Purpose::ToAList),
            THAT_ADDRESS_DOES_NOT_TAKE_LETTERS
        );
    }

    #[test]
    fn somebody_who_said_it_was_spam_is_written_to_about_nothing() {
        // Not a courtesy. The next letter of any sort is what has this site's
        // mail stopped for everybody else who reads it.
        assert_eq!(
            refused(Standing::Complained, Purpose::Because),
            THEY_SAID_THAT_WAS_SPAM
        );
        assert_eq!(
            refused(Standing::Complained, Purpose::ToAList),
            THEY_SAID_THAT_WAS_SPAM
        );
    }

    #[test]
    fn each_refusal_says_which_one_it_is() {
        let mut said = vec![
            refused(Standing::Unsubscribed, Purpose::ToAList),
            refused(Standing::Bounced, Purpose::ToAList),
            refused(Standing::Complained, Purpose::ToAList),
        ];
        let count = said.len();
        said.sort_unstable();
        said.dedup();

        assert_eq!(said.len(), count, "two of these say the same thing");
    }
}
