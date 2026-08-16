use serde::{Deserialize, Serialize};

use crate::{MaviError, Result};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
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
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
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
}
