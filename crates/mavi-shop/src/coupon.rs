//! Something off.
//!
//! Two kinds, one column, and the kind says which — a percentage or an amount.
//! What matters here is the arithmetic, because a discount that takes more off
//! than the order came to is a shop that owes somebody money.

use chrono::{DateTime, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::money::Money;
use mavi_core::say::Say;
use serde::{Deserialize, Serialize};

pub const A_CODE_IS_BETWEEN_THREE_AND_FORTY: &str = "a_code_is_between_three_and_forty";
pub const A_PERCENTAGE_IS_BETWEEN_ONE_AND_A_HUNDRED: &str =
    "a_percentage_is_between_one_and_a_hundred";
pub const SOMETHING_OFF_IS_MORE_THAN_NOTHING: &str = "something_off_is_more_than_nothing";
pub const THAT_CODE_HAS_RUN_OUT: &str = "that_code_has_run_out";
pub const THAT_CODE_HAS_EXPIRED: &str = "that_code_has_expired";
pub const THAT_CODE_IS_NOT_FOR_THIS_MONEY: &str = "that_code_is_not_for_this_money";

/// What sort of thing comes off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// So many per cent.
    Percent,
    /// So much money. In a currency, because "fifty off" is not a discount
    /// until something says fifty of what.
    Amount,
}

/// One code.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Coupon {
    /// Upper case, always. A code somebody reads off a poster and types in
    /// lower case is the same code, and the alternative is a shop whose
    /// discount works for half the people who try it.
    pub code: String,
    pub kind: Kind,
    /// A percentage, where the kind is `Percent`.
    pub percent: Option<u32>,
    /// An amount, where the kind is `Amount`.
    pub amount: Option<Money>,
    /// How many times it may be used at all. `None` is as many as anybody
    /// likes, which is a decision somebody made rather than a field left out.
    pub at_most_uses: Option<i64>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl Coupon {
    /// A percentage off.
    pub fn percent(code: &str, percent: u32) -> Result<Self> {
        if !(1..=100).contains(&percent) {
            return Err(Error::invalid(Say::of(
                A_PERCENTAGE_IS_BETWEEN_ONE_AND_A_HUNDRED,
            )));
        }

        Ok(Self {
            code: checked_code(code)?,
            kind: Kind::Percent,
            percent: Some(percent),
            amount: None,
            at_most_uses: None,
            expires_at: None,
        })
    }

    /// An amount off.
    pub fn amount(code: &str, amount: Money) -> Result<Self> {
        if amount.minor <= 0 {
            return Err(Error::invalid(Say::of(SOMETHING_OFF_IS_MORE_THAN_NOTHING)));
        }

        Ok(Self {
            code: checked_code(code)?,
            kind: Kind::Amount,
            percent: None,
            amount: Some(amount),
            at_most_uses: None,
            expires_at: None,
        })
    }
}

fn checked_code(code: &str) -> Result<String> {
    let code = code.trim().to_uppercase();

    let right = (3..=40).contains(&code.chars().count())
        && code
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-');

    if !right {
        return Err(Error::invalid(Say::of(A_CODE_IS_BETWEEN_THREE_AND_FORTY)));
    }

    Ok(code)
}

/// What comes off an order, and what is left.
///
/// Two rules, and both are the shop's money:
///
/// **Nothing comes off below zero.** Fifty lira off a thirty lira order is
/// thirty off, not fifty — the other twenty is not owed to anybody.
///
/// **A code in one currency does not come off an order in another.** Fifty
/// euros off a lira order is a number, and it is the wrong number by a factor
/// of about forty.
pub fn off(total: Money, coupon: &Coupon, used_so_far: i64, now: DateTime<Utc>) -> Result<Money> {
    if let Some(at_most) = coupon.at_most_uses
        && used_so_far >= at_most
    {
        return Err(Error::conflict(Say::of(THAT_CODE_HAS_RUN_OUT)));
    }

    if coupon.expires_at.is_some_and(|when| now >= when) {
        return Err(Error::conflict(Say::of(THAT_CODE_HAS_EXPIRED)));
    }

    let comes_off = match coupon.kind {
        Kind::Percent => {
            let percent = i64::from(coupon.percent.unwrap_or(0));

            // Down to the kuruş rather than up: rounding a discount up takes
            // money the shop did not agree to give away, and rounding it down
            // costs the customer at most one of the smallest coin there is.
            Money::of(total.minor.saturating_mul(percent) / 100, total.currency)
        }
        Kind::Amount => {
            let amount = coupon
                .amount
                .ok_or_else(|| Error::invalid(Say::of(SOMETHING_OFF_IS_MORE_THAN_NOTHING)))?;

            if amount.currency != total.currency {
                return Err(Error::invalid(Say::of(THAT_CODE_IS_NOT_FOR_THIS_MONEY)));
            }

            amount
        }
    };

    let left = total.minus(comes_off)?;

    Ok(if left.minor < 0 {
        Money::of(0, total.currency)
    } else {
        left
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use mavi_core::money::Currency;

    fn lira() -> Currency {
        Currency::parse("try").expect("a currency")
    }

    fn euro() -> Currency {
        Currency::parse("eur").expect("a currency")
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
    }

    fn refused(total: Money, coupon: &Coupon, used: i64) -> &'static str {
        off(total, coupon, used, now())
            .expect_err("a refusal")
            .said()
            .expect("a sentence")
            .key
    }

    #[test]
    fn a_code_is_the_same_code_however_it_was_typed() {
        // A shop whose discount works for half the people who try it is a
        // shop with a support queue.
        assert_eq!(
            Coupon::percent("spring-26", 10).expect("a code").code,
            "SPRING-26"
        );
    }

    #[test]
    fn nothing_comes_off_below_zero() {
        // Fifty off thirty is thirty off. The other twenty is not owed to
        // anybody, and a negative total is a shop that pays people to shop.
        let fifty_off = Coupon::amount("FIFTY", Money::of(5000, lira())).expect("a code");

        let left = off(Money::of(3000, lira()), &fifty_off, 0, now()).expect("a total");

        assert_eq!(left.minor, 0);
    }

    #[test]
    fn a_code_in_one_currency_does_not_come_off_an_order_in_another() {
        // Fifty euros off a lira order is a number, and it is wrong by about
        // a factor of forty.
        let euros = Coupon::amount("FIFTY", Money::of(5000, euro())).expect("a code");

        assert_eq!(
            refused(Money::of(300_000, lira()), &euros, 0),
            THAT_CODE_IS_NOT_FOR_THIS_MONEY
        );
    }

    #[test]
    fn a_percentage_rounds_the_shops_way_by_one_kurus() {
        // 33% of 10.00 is 3.30 exactly; 33% of 10.01 is 3.3033, and what comes
        // off is 3.30. The customer is out a third of a kuruş, which is the
        // smallest anybody can be out.
        let third = Coupon::percent("THIRD", 33).expect("a code");

        assert_eq!(
            off(Money::of(1001, lira()), &third, 0, now())
                .expect("a total")
                .minor,
            1001 - 330
        );
    }

    #[test]
    fn all_of_it_off_is_none_of_it_left() {
        let everything = Coupon::percent("EVERYTHING", 100).expect("a code");

        assert_eq!(
            off(Money::of(12_345, lira()), &everything, 0, now())
                .expect("a total")
                .minor,
            0
        );
    }

    #[test]
    fn a_code_that_has_run_out_or_expired_says_which() {
        let mut once = Coupon::percent("ONCE", 10).expect("a code");
        once.at_most_uses = Some(1);

        assert!(off(Money::of(1000, lira()), &once, 0, now()).is_ok());
        assert_eq!(
            refused(Money::of(1000, lira()), &once, 1),
            THAT_CODE_HAS_RUN_OUT
        );

        let mut over = Coupon::percent("OVER", 10).expect("a code");
        over.expires_at = Some(now());

        // Expiring *at* a moment means it is over at that moment, not still
        // going: "valid until noon" and a clock reading noon is an argument
        // nobody should have with a shop.
        assert_eq!(
            refused(Money::of(1000, lira()), &over, 0),
            THAT_CODE_HAS_EXPIRED
        );
    }

    #[test]
    fn a_percentage_is_a_percentage() {
        assert!(Coupon::percent("NONE", 0).is_err());
        assert!(Coupon::percent("MORE", 101).is_err());
        assert!(Coupon::amount("FREE", Money::of(0, lira())).is_err());
    }
}
