//! What a flow does, and what starts it.
//!
//! Both are closed lists. A flow that waits for an event nothing emits is a
//! flow that never runs and nobody is told about; a step of a kind nothing
//! knows how to do is a run that fails at the same place every time, once per
//! event, for as long as the flow exists.
//!
//! And every step's settings are checked **when the flow is written**. A step
//! that says "send a letter" and does not say which letter is not a mistake to
//! find at three in the morning, one failure per order.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::outward;

pub const NOTHING_HERE_HAPPENS_LIKE_THAT: &str = "nothing_here_happens_like_that";
pub const NOTHING_KNOWS_HOW_TO_DO_THAT: &str = "nothing_knows_how_to_do_that";
pub const THAT_STEP_NEEDS_TO_BE_TOLD_SOMETHING: &str = "that_step_needs_to_be_told_something";
pub const A_FLOW_DOES_SOMETHING: &str = "a_flow_does_something";
pub const A_FLOW_IS_AT_MOST_SO_LONG: &str = "a_flow_is_at_most_so_long";
pub const NOTHING_WAITS_THAT_LONG: &str = "nothing_waits_that_long";

/// How many steps one flow may have.
pub const AT_MOST_STEPS: usize = 20;

/// The longest a step may wait. A month, because a run that is waiting is a
/// row somebody has to keep, and one waiting a year is a row nobody remembers
/// agreeing to.
pub const AT_MOST_WAIT_HOURS: i64 = 24 * 31;

/// What can start a flow.
///
/// Each of these is emitted by a domain in this workspace, and the name is the
/// same name that domain's endpoint has. A trigger that is not here is refused
/// where the flow is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    SomethingWasPublished,
    SomebodyFilledInAForm,
    AnOrderWasPaidFor,
    AnOrderWentOut,
    SomebodyWasPutOnACourse,
    SomebodyFinishedACourse,
}

pub const TRIGGERS: &[Trigger] = &[
    Trigger::SomethingWasPublished,
    Trigger::SomebodyFilledInAForm,
    Trigger::AnOrderWasPaidFor,
    Trigger::AnOrderWentOut,
    Trigger::SomebodyWasPutOnACourse,
    Trigger::SomebodyFinishedACourse,
];

impl Trigger {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Trigger::SomethingWasPublished => "something_was_published",
            Trigger::SomebodyFilledInAForm => "somebody_filled_in_a_form",
            Trigger::AnOrderWasPaidFor => "an_order_was_paid_for",
            Trigger::AnOrderWentOut => "an_order_went_out",
            Trigger::SomebodyWasPutOnACourse => "somebody_was_put_on_a_course",
            Trigger::SomebodyFinishedACourse => "somebody_finished_a_course",
        }
    }

    pub fn parse(name: &str) -> Result<Self> {
        TRIGGERS
            .iter()
            .copied()
            .find(|trigger| trigger.as_str() == name)
            .ok_or_else(|| Error::invalid(Say::of(NOTHING_HERE_HAPPENS_LIKE_THAT)))
    }
}

/// What a step does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Does {
    /// Sends one of the site's own letters.
    SendALetter,
    /// Calls an address somebody else runs.
    CallAnAddress,
    /// Waits.
    Wait,
    /// Puts whoever this is about on one of the site's lists.
    PutOnAList,
}

impl Does {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Does::SendALetter => "send_a_letter",
            Does::CallAnAddress => "call_an_address",
            Does::Wait => "wait",
            Does::PutOnAList => "put_on_a_list",
        }
    }

    pub fn parse(name: &str) -> Result<Self> {
        [
            Does::SendALetter,
            Does::CallAnAddress,
            Does::Wait,
            Does::PutOnAList,
        ]
        .into_iter()
        .find(|does| does.as_str() == name)
        .ok_or_else(|| Error::invalid(Say::of(NOTHING_KNOWS_HOW_TO_DO_THAT)))
    }

    /// What this step has to be told before it can do anything.
    #[must_use]
    pub const fn needs(self) -> &'static [&'static str] {
        match self {
            Does::SendALetter => &["letter"],
            Does::CallAnAddress => &["address"],
            Does::Wait => &["hours"],
            Does::PutOnAList => &["list"],
        }
    }
}

/// One step of a flow, with its settings checked.
///
/// Nothing makes one of these except [`Step::checked`], so a step in hand is a
/// step that can run.
#[derive(Clone, Debug, Serialize)]
pub struct Step {
    pub does: Does,
    pub told: Value,
}

impl Step {
    pub fn checked(does: Does, told: &Value) -> Result<Self> {
        let object = told
            .as_object()
            .ok_or_else(|| Error::invalid(Say::of(THAT_STEP_NEEDS_TO_BE_TOLD_SOMETHING)))?;

        for needed in does.needs() {
            let there = object.get(*needed).is_some_and(|what| !what.is_null());

            if !there {
                return Err(Error::invalid(
                    Say::of(THAT_STEP_NEEDS_TO_BE_TOLD_SOMETHING)
                        .with("does", &does.as_str())
                        .with("needs", needed),
                ));
            }
        }

        match does {
            // The one that reaches out of this machine, so the one with a rule
            // about where it may reach.
            Does::CallAnAddress => {
                let address = object
                    .get("address")
                    .and_then(Value::as_str)
                    .unwrap_or_default();

                outward::to_call(address)?;
            }
            Does::Wait => {
                let hours = object.get("hours").and_then(Value::as_i64).unwrap_or(0);

                if !(1..=AT_MOST_WAIT_HOURS).contains(&hours) {
                    return Err(Error::invalid(
                        Say::of(NOTHING_WAITS_THAT_LONG).with("at_most", &AT_MOST_WAIT_HOURS),
                    ));
                }
            }
            Does::SendALetter | Does::PutOnAList => {}
        }

        Ok(Self {
            does,
            told: told.clone(),
        })
    }
}

/// A whole flow's steps, checked together.
pub fn all_of_them(steps: Vec<Step>) -> Result<Vec<Step>> {
    if steps.is_empty() {
        return Err(Error::invalid(Say::of(A_FLOW_DOES_SOMETHING)));
    }

    if steps.len() > AT_MOST_STEPS {
        return Err(Error::invalid(
            Say::of(A_FLOW_IS_AT_MOST_SO_LONG).with("at_most", &AT_MOST_STEPS),
        ));
    }

    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn refused(does: Does, told: &Value) -> &'static str {
        Step::checked(does, told)
            .expect_err("a refusal")
            .said()
            .expect("a sentence")
            .key
    }

    #[test]
    fn a_step_that_was_not_told_what_to_do_is_refused_where_it_is_written() {
        // Otherwise it is a run that fails in the same place every time, once
        // per order, and the first anybody hears of it is a customer asking
        // where their letter is.
        assert_eq!(
            refused(Does::SendALetter, &json!({})),
            THAT_STEP_NEEDS_TO_BE_TOLD_SOMETHING
        );

        assert!(Step::checked(Does::SendALetter, &json!({"letter": "order_paid"})).is_ok());
    }

    #[test]
    fn a_step_that_calls_somewhere_inside_this_machine_is_refused() {
        assert_eq!(
            refused(
                Does::CallAnAddress,
                &json!({"address": "http://localhost:5432"})
            ),
            outward::THAT_ADDRESS_IS_INSIDE_THIS_MACHINE
        );
    }

    #[test]
    fn nothing_waits_a_year() {
        // A run that is waiting is a row somebody has to keep.
        assert!(Step::checked(Does::Wait, &json!({"hours": 24})).is_ok());

        assert_eq!(
            refused(Does::Wait, &json!({"hours": 24 * 365})),
            NOTHING_WAITS_THAT_LONG
        );
        assert_eq!(
            refused(Does::Wait, &json!({"hours": 0})),
            NOTHING_WAITS_THAT_LONG
        );
    }

    #[test]
    fn a_trigger_nothing_emits_cannot_be_waited_for() {
        assert!(Trigger::parse("an_order_was_paid_for").is_ok());
        assert!(Trigger::parse("an_order_was_refunded").is_err());
    }

    #[test]
    fn a_flow_does_something_and_not_forty_things() {
        assert_eq!(
            all_of_them(Vec::new())
                .expect_err("a refusal")
                .said()
                .expect("a sentence")
                .key,
            A_FLOW_DOES_SOMETHING
        );

        let waiting = Step::checked(Does::Wait, &json!({"hours": 1})).expect("a step");
        let many = std::iter::repeat_n(waiting, AT_MOST_STEPS + 1).collect();

        assert_eq!(
            all_of_them(many)
                .expect_err("a refusal")
                .said()
                .expect("a sentence")
                .key,
            A_FLOW_IS_AT_MOST_SO_LONG
        );
    }
}
