//! An amount and what it is in.
//!
//! Held as a decimal rather than a float, because a float cannot hold `0.10`
//! and money that cannot hold a tenth is not money. Held with its currency
//! rather than beside it, because an amount without one is a number somebody
//! will add to a different currency eventually.
//!
//! The interesting operation is [`Money::split`]. Dividing a bill three ways
//! is where money stops behaving like arithmetic: a third of ten lira is not a
//! number of kuruş, and three of them must still be ten lira. So the parts are
//! handed out largest-remainder first and the total is checked, rather than
//! each part being rounded and the difference going wherever it goes.

use std::fmt;

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Which money. Three letters, as ISO 4217 writes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Currency(pub [u8; 3]);

impl Currency {
    pub fn parse(text: &str) -> Result<Self> {
        let bytes = text.as_bytes();

        if bytes.len() != 3 || !bytes.iter().all(u8::is_ascii_alphabetic) {
            return Err(Error::internal(std::io::Error::other(
                "a currency is three letters",
            )));
        }

        Ok(Self([
            bytes[0].to_ascii_uppercase(),
            bytes[1].to_ascii_uppercase(),
            bytes[2].to_ascii_uppercase(),
        ]))
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(std::str::from_utf8(&self.0).unwrap_or("???"))
    }
}

/// An amount of one currency.
///
/// Deliberately **not** ordered. Deriving `Ord` here was the first thing
/// written and the compiler refused it, which turned out to be the right
/// answer for a reason the derive would have hidden: an ordering makes
/// `Money::of(100, TRY) < Money::of(200, EUR)` compile and answer, and answer
/// something meaningless. It is the same mistake [`Money::plus`] refuses out
/// loud, arrived at silently through a sort.
///
/// Two amounts are compared with [`Money::over`], which refuses across
/// currencies the way addition does. Equality is derived and is safe: two
/// amounts in different currencies are simply not equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    /// In the currency's smallest unit — kuruş, cents, pence. An integer, so
    /// nothing is ever half a kuruş.
    pub minor: i64,
    pub currency: Currency,
}

impl Money {
    #[must_use]
    pub const fn of(minor: i64, currency: Currency) -> Self {
        Self { minor, currency }
    }

    /// Two amounts added, or a refusal. Adding lira to euros is not an
    /// arithmetic problem and answering zero would be worse than refusing.
    pub fn plus(self, other: Self) -> Result<Self> {
        self.same_as(other)?;

        Ok(Self {
            minor: self.minor.saturating_add(other.minor),
            currency: self.currency,
        })
    }

    pub fn minus(self, other: Self) -> Result<Self> {
        self.same_as(other)?;

        Ok(Self {
            minor: self.minor.saturating_sub(other.minor),
            currency: self.currency,
        })
    }

    /// Whether this is more than that, or a refusal. The ordering an `Ord`
    /// would have given for free, made to say what it cannot answer.
    pub fn over(self, other: Self) -> Result<bool> {
        self.same_as(other)?;

        Ok(self.minor > other.minor)
    }

    fn same_as(self, other: Self) -> Result<()> {
        if self.currency == other.currency {
            return Ok(());
        }

        Err(Error::internal(std::io::Error::other(format!(
            "{} and {} are not the same money",
            self.currency, other.currency
        ))))
    }

    /// This amount, so many times over.
    ///
    /// A line on an order is a price and a count, and this is the one place
    /// that multiplication happens. It refuses rather than saturating, unlike
    /// [`Money::plus`]: a sum that runs out of room is somebody adding up a
    /// shop's whole history, and a line that does is a number somebody is
    /// about to be charged.
    pub fn times(self, count: u32) -> Result<Self> {
        let minor = self
            .minor
            .checked_mul(i64::from(count))
            .ok_or_else(|| Error::internal(std::io::Error::other("more money than money goes")))?;

        Ok(Self {
            minor,
            currency: self.currency,
        })
    }

    /// This amount in `parts`, adding back up to exactly this amount.
    ///
    /// Ten lira in three is 3.34, 3.33, 3.33 — not three of 3.33 and a kuruş
    /// nobody can find. The remainder goes to the earliest parts, which is
    /// arbitrary and is written down here so that it is arbitrary *once*
    /// rather than differently in each caller.
    #[must_use]
    pub fn split(self, parts: usize) -> Vec<Self> {
        if parts == 0 {
            return Vec::new();
        }

        let Ok(count) = i64::try_from(parts) else {
            return Vec::new();
        };

        let each = self.minor / count;
        let over = self.minor % count;
        // How many parts get one more of the smallest unit, and in which
        // direction — a negative total is split into negative parts.
        let extra = over.unsigned_abs();
        let step = over.signum();

        (0..parts)
            .map(|i| Self {
                minor: each + i64::from(u64::try_from(i).is_ok_and(|i| i < extra)) * step,
                currency: self.currency,
            })
            .collect()
    }

    /// What a person reads: the major unit, with the currency's own number of
    /// places. Only for showing — nothing computes with this.
    #[must_use]
    pub fn to_decimal(self, places: u32) -> Decimal {
        Decimal::new(self.minor, places)
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}",
            self.to_decimal(2).to_f64().unwrap_or_default(),
            self.currency
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn try_() -> Currency {
        Currency::parse("try").expect("a currency")
    }

    #[test]
    fn what_is_split_adds_back_up() {
        for total in [1000_i64, 1, 0, 999_999, -1000] {
            for parts in 1..=7_usize {
                let money = Money::of(total, try_());
                let split = money.split(parts);

                assert_eq!(split.len(), parts);
                assert_eq!(
                    split.iter().map(|part| part.minor).sum::<i64>(),
                    total,
                    "{total} in {parts} lost or made a kuruş"
                );
            }
        }
    }

    #[test]
    fn two_currencies_are_not_ordered_either() {
        let lira = Money::of(100, try_());
        let euro = Money::of(200, Currency::parse("EUR").expect("a currency"));

        // The assertion that matters cannot be written, because it is that
        // `lira < euro` does not compile. What can be written is that the
        // comparison which replaced it refuses rather than answering.
        assert!(lira.over(euro).is_err(), "lira and euros were ordered");
        assert!(
            lira.over(Money::of(50, try_())).expect("the same money"),
            "a hundred was not more than fifty"
        );
    }

    #[test]
    fn two_currencies_are_not_added() {
        let lira = Money::of(100, try_());
        let euro = Money::of(100, Currency::parse("EUR").expect("a currency"));

        assert!(
            lira.plus(euro).is_err(),
            "lira and euros added to something"
        );
    }

    #[test]
    fn a_currency_is_three_letters_however_it_was_written() {
        assert_eq!(
            Currency::parse("try").expect("a currency").to_string(),
            "TRY"
        );
        assert!(Currency::parse("TRYX").is_err());
        assert!(Currency::parse("T9Y").is_err());
    }

    #[test]
    fn a_line_is_a_price_so_many_times_over() {
        let each = Money::of(1250, try_());

        assert_eq!(each.times(3).expect("three of them").minor, 3750);
        assert_eq!(each.times(0).expect("none of them").minor, 0);
    }

    #[test]
    fn more_money_than_money_goes_is_refused_rather_than_rounded_down() {
        // Saturating here would be a number somebody is charged, arrived at by
        // giving up.
        let vast = Money::of(i64::MAX / 2, try_());

        assert!(vast.times(3).is_err());
    }
}
