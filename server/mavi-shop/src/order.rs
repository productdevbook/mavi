//! An order, and what may happen to it next.
//!
//! An order is the one thing here that somebody else's money has been through,
//! so what it may do is written down as a machine rather than left to whichever
//! handler is being written today.

use mavi_core::error::{Error, Result};
use mavi_core::money::{Currency, Money};
use mavi_core::say::Say;
use serde::{Deserialize, Serialize};

pub const AN_ORDER_DOES_NOT_GO_BACK: &str = "an_order_does_not_go_back";
pub const THAT_IS_NOT_WHERE_AN_ORDER_GOES_NEXT: &str = "that_is_not_where_an_order_goes_next";
pub const AN_ORDER_IS_IN_ONE_CURRENCY: &str = "an_order_is_in_one_currency";
pub const AN_ORDER_HAS_SOMETHING_IN_IT: &str = "an_order_has_something_in_it";
pub const NOT_THAT_MANY_OF_ANYTHING: &str = "not_that_many_of_anything";

/// The most of one thing anybody buys at once.
///
/// A limit exists because the arithmetic beyond it is a number nobody meant,
/// and because a shop with one of something on the shelf being asked for four
/// billion is somebody trying it on.
pub const AT_MOST_EACH: u32 = 1000;

/// Where an order is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Made, and not paid for. Stock is held against it.
    Waiting,
    Paid,
    /// Gone out. Whatever that means for what this shop sells.
    Sent,
    /// Never paid for, or called off before it went.
    CalledOff,
    /// Paid for and given back.
    GivenBack,
}

impl State {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            State::Waiting => "waiting",
            State::Paid => "paid",
            State::Sent => "sent",
            State::CalledOff => "called_off",
            State::GivenBack => "given_back",
        }
    }

    /// Whether the money has been taken by this point.
    ///
    /// Asked as one question rather than as a list of states written out at
    /// each place that needs it — which is how a list ends up missing the
    /// state somebody added last week.
    #[must_use]
    pub const fn money_was_taken(self) -> bool {
        matches!(self, State::Paid | State::Sent | State::GivenBack)
    }

    /// Whether nothing further happens to it.
    #[must_use]
    pub const fn is_the_end(self) -> bool {
        matches!(self, State::CalledOff | State::GivenBack)
    }
}

/// Whether an order may go from here to there.
///
/// Written as a machine because the alternative is each handler asking "is it
/// paid yet" in its own words, and the one that forgets is the one that sends
/// something nobody paid for.
pub fn moves(from: State, to: State) -> Result<()> {
    let allowed = matches!(
        (from, to),
        (State::Waiting, State::Paid | State::CalledOff)
            | (
                State::Paid,
                State::Sent | State::CalledOff | State::GivenBack
            )
            | (State::Sent, State::GivenBack)
    );

    if allowed {
        return Ok(());
    }

    // Two refusals rather than one: going backwards is a different mistake
    // from going somewhere that does not follow, and whoever reads it can act
    // on the difference.
    let said = if from.is_the_end() {
        AN_ORDER_DOES_NOT_GO_BACK
    } else {
        THAT_IS_NOT_WHERE_AN_ORDER_GOES_NEXT
    };

    Err(Error::conflict(
        Say::of(said)
            .with("from", &from.as_str())
            .with("to", &to.as_str()),
    ))
}

/// One line of an order: what it was called and what it cost **at the time**.
///
/// A price that changes next week does not change what somebody was charged
/// last week, which is why this is a copy rather than a reference to the
/// product's own price.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Line {
    pub name: String,
    pub each: Money,
    pub how_many: u32,
}

impl Line {
    pub fn comes_to(&self) -> Result<Money> {
        self.each.times(self.how_many)
    }
}

/// What an order comes to.
///
/// Refuses an empty order and refuses one in two currencies. The second is not
/// a hypothetical: a shop that sells in lira and adds one product priced in
/// euros has an order whose total is a number in neither.
pub fn comes_to(lines: &[Line]) -> Result<Money> {
    let Some(first) = lines.first() else {
        return Err(Error::invalid(Say::of(AN_ORDER_HAS_SOMETHING_IN_IT)));
    };

    let mut total = Money::of(0, first.each.currency);

    for line in lines {
        if line.each.currency != total.currency {
            return Err(Error::invalid(Say::of(AN_ORDER_IS_IN_ONE_CURRENCY)));
        }

        if line.how_many == 0 || line.how_many > AT_MOST_EACH {
            return Err(Error::invalid(
                Say::of(NOT_THAT_MANY_OF_ANYTHING).with("at_most", &AT_MOST_EACH),
            ));
        }

        total = total.plus(line.comes_to()?)?;
    }

    Ok(total)
}

/// What currency an order is in, where there is one.
#[must_use]
pub fn one_currency(lines: &[Line]) -> Option<Currency> {
    lines.first().map(|line| line.each.currency)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lira() -> Currency {
        Currency::parse("try").expect("a currency")
    }

    fn euro() -> Currency {
        Currency::parse("eur").expect("a currency")
    }

    fn line(each: i64, how_many: u32, currency: Currency) -> Line {
        Line {
            name: "A Thing".to_owned(),
            each: Money::of(each, currency),
            how_many,
        }
    }

    fn refused(from: State, to: State) -> &'static str {
        moves(from, to)
            .expect_err("a refusal")
            .said()
            .expect("a sentence")
            .key
    }

    #[test]
    fn nothing_is_sent_that_was_not_paid_for() {
        // The whole reason this is a machine rather than a question each
        // handler asks in its own words.
        assert_eq!(
            refused(State::Waiting, State::Sent),
            THAT_IS_NOT_WHERE_AN_ORDER_GOES_NEXT
        );

        assert!(moves(State::Waiting, State::Paid).is_ok());
        assert!(moves(State::Paid, State::Sent).is_ok());
    }

    #[test]
    fn nothing_is_given_back_that_was_never_paid_for() {
        assert_eq!(
            refused(State::Waiting, State::GivenBack),
            THAT_IS_NOT_WHERE_AN_ORDER_GOES_NEXT
        );
    }

    #[test]
    fn an_order_that_is_finished_is_finished() {
        for from in [State::CalledOff, State::GivenBack] {
            for to in [State::Waiting, State::Paid, State::Sent] {
                assert_eq!(refused(from, to), AN_ORDER_DOES_NOT_GO_BACK);
            }
        }
    }

    #[test]
    fn where_the_money_is_asked_once() {
        // A list of states written out at each place that needs it is a list
        // that misses whichever state was added last week.
        assert!(!State::Waiting.money_was_taken());
        assert!(!State::CalledOff.money_was_taken());
        assert!(State::Paid.money_was_taken());
        assert!(State::Sent.money_was_taken());
        assert!(State::GivenBack.money_was_taken());
    }

    #[test]
    fn an_order_in_two_currencies_has_no_total() {
        // Not a hypothetical: a shop selling in lira that adds a product
        // priced in euros has an order whose total is a number in neither.
        let mixed = vec![line(1000, 1, lira()), line(1000, 1, euro())];

        assert_eq!(
            comes_to(&mixed)
                .expect_err("a refusal")
                .said()
                .expect("a sentence")
                .key,
            AN_ORDER_IS_IN_ONE_CURRENCY
        );
    }

    #[test]
    fn an_order_of_nothing_is_not_an_order() {
        assert_eq!(
            comes_to(&[])
                .expect_err("a refusal")
                .said()
                .expect("a sentence")
                .key,
            AN_ORDER_HAS_SOMETHING_IN_IT
        );
    }

    #[test]
    fn what_it_comes_to_is_what_the_lines_say() {
        let order = vec![line(1250, 3, lira()), line(500, 2, lira())];

        assert_eq!(comes_to(&order).expect("a total").minor, 4750);
    }

    #[test]
    fn nobody_buys_four_billion_of_anything() {
        let greedy = vec![line(1, AT_MOST_EACH + 1, lira())];

        assert_eq!(
            comes_to(&greedy)
                .expect_err("a refusal")
                .said()
                .expect("a sentence")
                .key,
            NOT_THAT_MANY_OF_ANYTHING
        );
    }
}
