use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{MaviError, Result};

/// A normalized address that is safe to place in a mail recipient field.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Email(String);

impl Email {
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim().to_ascii_lowercase();
        let Some((local, domain)) = value.split_once('@') else {
            return Err(MaviError::validation("invalid_email"));
        };
        let valid_local = !local.is_empty()
            && local.len() <= 64
            && !local.starts_with('.')
            && !local.ends_with('.')
            && !local.contains("..")
            && local.chars().all(|character| {
                character.is_ascii_alphanumeric()
                    || matches!(character, '.' | '_' | '%' | '+' | '-')
            });
        let valid_domain = domain.len() <= 255
            && domain.contains('.')
            && domain.split('.').all(|part| {
                !part.is_empty()
                    && part.len() <= 63
                    && !part.starts_with('-')
                    && !part.ends_with('-')
                    && part
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '-')
            });
        if !valid_local || !valid_domain {
            return Err(MaviError::validation("invalid_email"));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn domain(&self) -> &str {
        self.0.rsplit_once('@').map_or("", |(_, domain)| domain)
    }
}

impl fmt::Display for Email {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_normalized_and_header_injection_is_rejected() {
        assert_eq!(
            Email::parse("  Some.One+tag@Example.Test ")
                .unwrap()
                .as_str(),
            "some.one+tag@example.test"
        );
        assert!(Email::parse("somebody@example.test\nBcc: nobody@example.test").is_err());
        assert!(Email::parse("not-an-address").is_err());
        assert!(Email::parse("somebody@-example.test").is_err());
    }
}
