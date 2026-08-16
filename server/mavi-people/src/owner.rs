//! The rule that keeps somebody able to get in.
//!
//! A site with no owner is a site nobody can change: not the design, not the
//! accounts, not the thing that is wrong. Getting back in means a shell on the
//! machine, and whoever needed it is usually the one person who does not have
//! one.
//!
//! This is the bug that was found by counting rather than by reading. The
//! crate this replaces had no check anywhere: `DELETE /api/people/{id}` would
//! remove the only owner, and so would suspending them, and so would moving
//! them to another role. Each of the four doors looked fine on its own.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;

pub const SOMEBODY_HAS_TO_BE_ABLE_TO_GET_IN: &str = "somebody_has_to_be_able_to_get_in";

/// What is being done to somebody who holds the owner's role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Doing {
    Removing,
    Stopping,
    /// Moving them to a role that is not the owner's.
    MovingThem,
    /// Taking the owner's role itself away.
    TakingTheRole,
}

impl Doing {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Doing::Removing => "removing",
            Doing::Stopping => "stopping",
            Doing::MovingThem => "moving_them",
            Doing::TakingTheRole => "taking_the_role",
        }
    }
}

/// Whether this may be done.
///
/// `others` is how many **other** people hold the owner's role and can still
/// sign in — not how many rows exist. Somebody who was invited and never took
/// it up cannot get in, and counting them is how a site ends up locked with an
/// owner who has never had a password.
///
/// One function for all four doors, because four checks written at four call
/// sites is three of them written and one forgotten.
pub fn may(doing: Doing, others_who_can_get_in: usize) -> Result<()> {
    if others_who_can_get_in > 0 {
        return Ok(());
    }

    Err(Error::conflict(
        Say::of(SOMEBODY_HAS_TO_BE_ABLE_TO_GET_IN).with("doing", &doing.as_str()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_owner_stays_however_they_are_being_taken_away() {
        // Four doors, one rule. Each of the four read as obviously fine on its
        // own, which is why three of them had no check at all.
        for doing in [
            Doing::Removing,
            Doing::Stopping,
            Doing::MovingThem,
            Doing::TakingTheRole,
        ] {
            let refused = may(doing, 0).expect_err("a refusal");
            let said = refused.said().expect("a sentence");

            assert_eq!(said.key, SOMEBODY_HAS_TO_BE_ABLE_TO_GET_IN);
            // Which of the four it was, because "no" alone leaves somebody
            // trying the next door.
            assert_eq!(
                said.named.get("doing").map(String::as_str),
                Some(doing.as_str())
            );
        }
    }

    #[test]
    fn one_of_several_owners_goes_without_trouble() {
        for doing in [Doing::Removing, Doing::Stopping, Doing::MovingThem] {
            assert!(may(doing, 1).is_ok());
        }
    }

    #[test]
    fn somebody_who_never_took_up_their_invitation_does_not_count() {
        // The counting is the caller's, and this is where it is written down:
        // an owner who has never had a password cannot get in, so a site whose
        // only other owner is one of those is a site nobody can get into.
        //
        // Expressed here as the argument's own name — `others_who_can_get_in`
        // — and as this test, which is what somebody reads before writing the
        // query.
        assert!(may(Doing::Removing, 0).is_err());
    }
}
