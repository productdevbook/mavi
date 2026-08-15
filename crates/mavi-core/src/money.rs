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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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

    fn same_as(self, other: Self) -> Result<()> {
        if self.currency == other.currency {
            return Ok(());
        }

        Err(Error::internal(std::io::Error::other(format!(
            "{} and {} are not the same money",
            self.currency, other.currency
        ))))
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

        let each = self.minor / parts as i64;
        let over = self.minor % parts as i64;

        (0..parts)
            .map(|i| Self {
                minor: each + i64::from(i < over.unsigned_abs() as usize) * over.signum(),
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
    fn two_currencies_are_not_added() {
        let lira = Money::of(100, try_());
        let euro = Money::of(100, Currency::parse("EUR").expect("a currency"));

        assert!(lira.plus(euro).is_err(), "lira and euros added to something");
    }

    #[test]
    fn a_currency_is_three_letters_however_it_was_written() {
        assert_eq!(Currency::parse("try").expect("a currency").to_string(), "TRY");
        assert!(Currency::parse("TRYX").is_err());
        assert!(Currency::parse("T9Y").is_err());
    }
}
