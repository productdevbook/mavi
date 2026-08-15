//! Amounts, in the smallest unit and never a float.
//!
//! Money is an integer and a currency, together, so an amount cannot be added
//! to one in another currency by accident. Nothing here rounds: what a shop
//! charged is what a shop charged.
use serde::{Deserialize, Serialize};

use super::error::{AppError, Result};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "currency", rename_all = "UPPERCASE")]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    Try,
    Eur,
    Usd,
    Gbp,
}

impl Currency {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Currency::Try => "TRY",
            Currency::Eur => "EUR",
            Currency::Usd => "USD",
            Currency::Gbp => "GBP",
        }
    }

    /// Every currency here has two. The type exists so that the day one does
    /// not, the answer is in a function rather than in a hundred `* 100`s.
    #[must_use]
    pub fn minor_digits(self) -> u32 {
        2
    }
}

impl std::fmt::Display for Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An amount in minor units. There is no conversion from a float, in either
/// direction, so a price cannot arrive through one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Money {
    pub minor: i64,
    pub currency: Currency,
}

impl Money {
    #[must_use]
    pub const fn new(minor: i64, currency: Currency) -> Self {
        Self { minor, currency }
    }

    #[must_use]
    pub const fn zero(currency: Currency) -> Self {
        Self::new(0, currency)
    }

    pub fn plus(self, other: Self) -> Result<Self> {
        self.same_currency(other)?;

        self.minor
            .checked_add(other.minor)
            .map(|minor| Self::new(minor, self.currency))
            .ok_or(AppError::Bug("an amount overflowed"))
    }

    pub fn minus(self, other: Self) -> Result<Self> {
        self.same_currency(other)?;

        self.minor
            .checked_sub(other.minor)
            .map(|minor| Self::new(minor, self.currency))
            .ok_or(AppError::Bug("an amount overflowed"))
    }

    pub fn times(self, quantity: u32) -> Result<Self> {
        self.minor
            .checked_mul(i64::from(quantity))
            .map(|minor| Self::new(minor, self.currency))
            .ok_or(AppError::Bug("an amount overflowed"))
    }

    fn same_currency(self, other: Self) -> Result<()> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(AppError::Bug("two currencies were added together"))
        }
    }
}

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let scale = 10_i64.pow(self.currency.minor_digits());
        let sign = if self.minor < 0 { "-" } else { "" };
        let minor = self.minor.unsigned_abs();

        write!(
            f,
            "{sign}{}.{:0width$} {}",
            minor / scale.unsigned_abs(),
            minor % scale.unsigned_abs(),
            self.currency,
            width = self.currency.minor_digits() as usize
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_currencies_do_not_add_up() {
        let lira = Money::new(1000, Currency::Try);
        let euro = Money::new(1000, Currency::Eur);

        assert!(lira.plus(euro).is_err());
        assert_eq!(
            lira.plus(lira).expect("adds"),
            Money::new(2000, Currency::Try)
        );
    }

    #[test]
    fn an_amount_that_would_overflow_is_an_error_rather_than_a_wrap() {
        let huge = Money::new(i64::MAX, Currency::Usd);

        assert!(huge.plus(Money::new(1, Currency::Usd)).is_err());
        assert!(huge.times(2).is_err());
    }

    #[test]
    fn it_reads_the_way_a_price_is_written() {
        assert_eq!(Money::new(1234, Currency::Try).to_string(), "12.34 TRY");
        assert_eq!(Money::new(-5, Currency::Eur).to_string(), "-0.05 EUR");
        assert_eq!(Money::new(0, Currency::Usd).to_string(), "0.00 USD");
    }
}
