use serde::{Deserialize, Deserializer, Serialize};

use crate::{MaviError, Result};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Cursor(String);

impl Cursor {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() || value.len() > 512 {
            return Err(MaviError::validation("invalid_cursor"));
        }

        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PageRequest {
    pub after: Option<Cursor>,
    #[serde(default, deserialize_with = "deserialize_optional_limit")]
    pub limit: Option<u16>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LimitValue {
    Number(u16),
    String(String),
}

fn deserialize_optional_limit<'de, D>(deserializer: D) -> std::result::Result<Option<u16>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<LimitValue>::deserialize(deserializer)?;
    value
        .map(|value| match value {
            LimitValue::Number(limit) => Ok(limit),
            LimitValue::String(limit) => limit
                .parse::<u16>()
                .map_err(|_| serde::de::Error::custom("limit must be an unsigned 16-bit integer")),
        })
        .transpose()
}

impl PageRequest {
    pub const DEFAULT_LIMIT: u16 = 25;
    pub const MAX_LIMIT: u16 = 100;

    #[must_use]
    pub const fn effective_limit(&self) -> u16 {
        match self.limit {
            Some(limit) if limit < 1 => 1,
            Some(limit) if limit > Self::MAX_LIMIT => Self::MAX_LIMIT,
            Some(limit) => limit,
            None => Self::DEFAULT_LIMIT,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<Cursor>,
}

impl<T> Page<T> {
    #[must_use]
    pub const fn new(items: Vec<T>, next_cursor: Option<Cursor>) -> Self {
        Self { items, next_cursor }
    }
}
