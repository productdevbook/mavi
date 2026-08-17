use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as DeError};

use crate::{MaviError, Result};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Currency([u8; 3]);

impl Currency {
    pub fn parse(value: &str) -> Result<Self> {
        let bytes = value.as_bytes();
        if bytes.len() != 3 || !bytes.iter().all(u8::is_ascii_uppercase) {
            return Err(MaviError::validation("currency_uppercase_iso_code"));
        }

        Ok(Self([bytes[0], bytes[1], bytes[2]]))
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 3] {
        &self.0
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        // Currency is constructed only from exactly three ASCII uppercase
        // bytes, so this conversion cannot fail.
        std::str::from_utf8(&self.0).expect("currency is ASCII")
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Currency {
    type Err = MaviError;

    fn from_str(value: &str) -> Result<Self> {
        Self::parse(value)
    }
}

impl Serialize for Currency {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(D::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct Money {
    pub minor: i64,
    pub currency: Currency,
}

impl Money {
    pub fn new(minor: i64, currency: Currency) -> Result<Self> {
        if minor < 0 {
            return Err(MaviError::validation("money_must_not_be_negative"));
        }

        Ok(Self { minor, currency })
    }

    pub fn plus(self, other: Self) -> Result<Self> {
        ensure_same_currency(self, other)?;
        let minor = self
            .minor
            .checked_add(other.minor)
            .ok_or_else(|| MaviError::validation("money_overflow"))?;
        Self::new(minor, self.currency)
    }

    pub fn subtract_floor(self, other: Self) -> Result<Self> {
        ensure_same_currency(self, other)?;
        let minor = self
            .minor
            .checked_sub(other.minor)
            .ok_or_else(|| MaviError::validation("money_overflow"))?;
        Self::new(minor.max(0), self.currency)
    }

    pub fn times(self, quantity: u32) -> Result<Self> {
        let minor = self
            .minor
            .checked_mul(i64::from(quantity))
            .ok_or_else(|| MaviError::validation("money_overflow"))?;
        Self::new(minor, self.currency)
    }
}

impl<'de> Deserialize<'de> for Money {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct MoneyFields {
            minor: i64,
            currency: Currency,
        }

        let fields = MoneyFields::deserialize(deserializer)?;
        Self::new(fields.minor, fields.currency).map_err(D::Error::custom)
    }
}

fn ensure_same_currency(left: Money, right: Money) -> Result<()> {
    if left.currency != right.currency {
        return Err(MaviError::validation("money_currency_mismatch"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_serializes_as_minor_units_and_an_iso_currency() {
        let money = Money::new(1_250, Currency::parse("TRY").expect("currency")).expect("money");
        assert_eq!(
            serde_json::to_value(money).expect("json"),
            serde_json::json!({
                "minor": 1250,
                "currency": "TRY"
            })
        );
        assert_eq!(money.plus(money).expect("sum").minor, 2500);
        assert_eq!(money.times(2).expect("product").minor, 2500);
    }

    #[test]
    fn money_rejects_negative_and_mixed_currency_values() {
        let currency = Currency::parse("TRY").expect("currency");
        let error = Money::new(-1, currency).expect_err("negative money");
        assert!(matches!(
            error,
            MaviError::Validation { code, .. } if code == "money_must_not_be_negative"
        ));

        let serde_error = serde_json::from_value::<Money>(serde_json::json!({
            "minor": -1,
            "currency": "TRY"
        }))
        .expect_err("negative money");
        assert!(serde_error.is_data());

        let try_money = Money::new(100, Currency::parse("TRY").expect("currency")).expect("money");
        let eur_money = Money::new(100, Currency::parse("EUR").expect("currency")).expect("money");
        assert!(try_money.plus(eur_money).is_err());
    }
}
