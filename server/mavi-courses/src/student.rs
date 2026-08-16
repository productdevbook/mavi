//! Somebody learning here.
//!
//! A student is not an account. They sign in at the site's own front, hold no
//! grants, and reach nothing in the panel — and that is why they are a
//! separate kind of caller rather than an account with everything switched
//! off. An account with everything switched off is one flag away from an
//! account with something switched on.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use serde::{Deserialize, Serialize};

pub const THAT_ACCOUNT_HAS_NOT_BEEN_TAKEN_UP: &str = "that_account_has_not_been_taken_up";
pub const THAT_ACCOUNT_IS_STOPPED: &str = "that_account_is_stopped";
pub const THAT_COURSE_IS_NOT_OPEN: &str = "that_course_is_not_open";
pub const THAT_IS_NOT_A_COURSE_THEY_ARE_ON: &str = "that_is_not_a_course_they_are_on";

/// Where somebody stands with the site they are learning at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    /// Written to, and has not chosen a password yet.
    Asked,
    Learning,
    /// Stopped by whoever runs the site. Their work is kept: stopping somebody
    /// is not the same as forgetting them, and a site that confuses the two
    /// cannot undo a mistake.
    Stopped,
}

impl Standing {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Standing::Asked => "asked",
            Standing::Learning => "learning",
            Standing::Stopped => "stopped",
        }
    }
}

/// Whether somebody standing like this may sign in.
///
/// Two refusals, because they are two different things to be told. One is
/// "your invitation is still waiting" and the other is "somebody here stopped
/// this", and a single "no" leaves whoever is reading it pressing the button
/// again.
pub fn may_sign_in(standing: Standing) -> Result<()> {
    match standing {
        Standing::Learning => Ok(()),
        Standing::Asked => Err(Error::forbidden(Say::of(
            THAT_ACCOUNT_HAS_NOT_BEEN_TAKEN_UP,
        ))),
        Standing::Stopped => Err(Error::forbidden(Say::of(THAT_ACCOUNT_IS_STOPPED))),
    }
}

/// Whether a signed-in student may open this lesson.
///
/// Three things have to be true and each is asked here rather than in the
/// handler: they are still learning, they are on the course, and the course is
/// open. In the crate this replaces the last one was asked in one of the two
/// places a lesson could be read from, so a student who had the address of a
/// lesson in a course that had been closed could still open it.
pub fn may_open(standing: Standing, on_the_course: bool, course_is_open: bool) -> Result<()> {
    may_sign_in(standing)?;

    if !on_the_course {
        // Not `not found`: they asked about a course that exists and is not
        // theirs, and telling them it does not exist teaches them nothing
        // while making the real "no such course" mean two things.
        return Err(Error::forbidden(Say::of(THAT_IS_NOT_A_COURSE_THEY_ARE_ON)));
    }

    if !course_is_open {
        return Err(Error::forbidden(Say::of(THAT_COURSE_IS_NOT_OPEN)));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(standing: Standing, on_the_course: bool, open: bool) -> &'static str {
        may_open(standing, on_the_course, open)
            .expect_err("a refusal")
            .said()
            .expect("a sentence")
            .key
    }

    #[test]
    fn somebody_learning_on_an_open_course_gets_in() {
        assert!(may_open(Standing::Learning, true, true).is_ok());
    }

    #[test]
    fn a_closed_course_is_closed_from_every_direction() {
        // The hole this fills: the check for a closed course lived in one of
        // the two places a lesson could be read from, so anybody holding the
        // address of a lesson could still open it.
        assert_eq!(
            refused(Standing::Learning, true, false),
            THAT_COURSE_IS_NOT_OPEN
        );
    }

    #[test]
    fn being_on_a_course_is_not_the_same_as_being_signed_in() {
        assert_eq!(
            refused(Standing::Learning, false, true),
            THAT_IS_NOT_A_COURSE_THEY_ARE_ON
        );
    }

    #[test]
    fn each_way_of_being_refused_says_which_it_is() {
        // "No" on its own has whoever is reading it pressing the button again.
        let keys = [
            refused(Standing::Asked, true, true),
            refused(Standing::Stopped, true, true),
            refused(Standing::Learning, false, true),
            refused(Standing::Learning, true, false),
        ];

        let mut all = keys.to_vec();
        all.sort_unstable();
        all.dedup();

        assert_eq!(all.len(), keys.len(), "two refusals say the same thing");
    }
}
