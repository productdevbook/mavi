//! One journey through a flow.
//!
//! A run holds what set it off **as it was at the time**, and reads that
//! rather than going back to the row. The row may have changed, and it may be
//! gone: a flow that sends a receipt an hour after an order is a flow whose
//! order can be refunded in the meantime, and a receipt about the refunded
//! version is a letter nobody meant to send.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use serde::{Deserialize, Serialize};

pub const THAT_RUN_IS_FINISHED: &str = "that_run_is_finished";

/// Where a run is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Working through the steps.
    Going,
    /// Between steps, on purpose: something said to wait.
    Waiting,
    Done,
    /// A step failed for good. The run stops where it is, which is what makes
    /// it possible to see what happened rather than what would have happened.
    Stuck,
}

impl State {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            State::Going => "going",
            State::Waiting => "waiting",
            State::Done => "done",
            State::Stuck => "stuck",
        }
    }

    #[must_use]
    pub const fn is_the_end(self) -> bool {
        matches!(self, State::Done | State::Stuck)
    }
}

/// What one step did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Went {
    /// Done, carry on.
    On,
    /// Done, and the run waits before the next one.
    Waited,
    /// Failed, and worth another go — a mail host that was busy, an address
    /// that answered with a five hundred.
    Again,
    /// Failed, and not worth another go: whatever it was told to do cannot be
    /// done at all.
    NoUse,
}

/// Where the run goes after a step went like that.
///
/// One function, because "what happens after a step" asked in two places is
/// two answers — and the second one is the one that leaves a run going round
/// for ever.
pub fn next_step(at: i32, how_many: i32, went: Went) -> Result<(State, i32)> {
    let after = at.saturating_add(1);

    Ok(match went {
        Went::On if after >= how_many => (State::Done, after),
        Went::On => (State::Going, after),
        Went::Waited => (State::Waiting, after),
        // Stays where it is: the same step is what runs next.
        Went::Again => (State::Going, at),
        Went::NoUse => (State::Stuck, at),
    })
}

/// Whether a run may be pushed along at all.
pub fn may_carry_on(state: State) -> Result<()> {
    if state.is_the_end() {
        return Err(Error::conflict(
            Say::of(THAT_RUN_IS_FINISHED).with("state", &state.as_str()),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_that_reaches_the_end_of_its_steps_is_done() {
        assert_eq!(next_step(2, 3, Went::On).expect("on"), (State::Done, 3));
        assert_eq!(next_step(0, 3, Went::On).expect("on"), (State::Going, 1));
    }

    #[test]
    fn a_step_worth_another_go_stays_where_it_is() {
        // Moving on after a failure is how a flow "sends" a letter that never
        // went anywhere.
        assert_eq!(
            next_step(1, 3, Went::Again).expect("again"),
            (State::Going, 1)
        );
    }

    #[test]
    fn a_step_that_cannot_be_done_stops_the_run_where_it_is() {
        // Stopping where it is rather than at the end is what makes it
        // possible to see what happened instead of what would have happened.
        assert_eq!(
            next_step(1, 3, Went::NoUse).expect("no use"),
            (State::Stuck, 1)
        );
    }

    #[test]
    fn a_run_that_is_finished_is_not_pushed_along() {
        assert!(may_carry_on(State::Going).is_ok());
        assert!(may_carry_on(State::Waiting).is_ok());

        for over in [State::Done, State::Stuck] {
            assert_eq!(
                may_carry_on(over)
                    .expect_err("a refusal")
                    .said()
                    .expect("a sentence")
                    .key,
                THAT_RUN_IS_FINISHED
            );
        }
    }
}
