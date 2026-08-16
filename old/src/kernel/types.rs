//! The small types a site's own words arrive as.
//!
//! A title that is not empty, a slug that is an address, an address that is an
//! address. Parsed at the edge and carried as themselves afterwards, so that
//! the checking happens once rather than in every handler that touches one.
use serde::{Deserialize, Serialize};

use super::error::{AppError, Result};
use crate::kernel::say;

macro_rules! parsed_string {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            #[must_use]
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl utoipa::PartialSchema for $name {
            fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::Schema> {
                utoipa::openapi::ObjectBuilder::new()
                    .schema_type(utoipa::openapi::schema::SchemaType::Type(
                        utoipa::openapi::Type::String,
                    ))
                    .description(Some(concat!(
                        "A ",
                        stringify!($name),
                        ", parsed on the way in rather than checked later."
                    )))
                    .into()
            }
        }

        impl utoipa::ToSchema for $name {}

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> std::result::Result<Self, D::Error> {
                let raw = String::deserialize(deserializer)?;
                Self::parse(&raw).map_err(serde::de::Error::custom)
            }
        }
    };
}

parsed_string!(Email);
parsed_string!(Slug);
parsed_string!(Title);

impl Email {
    /// Lowercased on the way in, so two spellings of one address are one
    /// address everywhere below this.
    pub fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim().to_ascii_lowercase();

        let looks_like = trimmed.split_once('@').is_some_and(|(name, domain)| {
            !name.is_empty() && domain.contains('.') && !domain.starts_with('.')
        });

        if !looks_like || trimmed.len() > 320 || trimmed.contains(char::is_whitespace) {
            return Err(AppError::Invalid(say::NOT_EMAIL_ADDRESS.into()));
        }

        Ok(Self(trimmed))
    }
}

impl Slug {
    pub fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim().to_ascii_lowercase();

        let shaped = !trimmed.is_empty()
            && trimmed.len() <= 200
            && trimmed
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !trimmed.starts_with('-')
            && !trimmed.ends_with('-')
            && !trimmed.contains("--");

        if !shaped {
            return Err(AppError::Invalid(
                say::SLUG_LOWERCASE_LETTERS_DIGITS_SINGLE_HYPHENS.into(),
            ));
        }

        Ok(Self(trimmed))
    }

    /// Turkish letters carry to their ASCII neighbours rather than being
    /// dropped, so a title in Turkish does not become an empty slug.
    #[must_use]
    pub fn from_title(title: &str) -> Self {
        let mut out = String::with_capacity(title.len());

        for c in title.to_lowercase().chars() {
            let mapped = match c {
                'ı' | 'i' | 'î' => "i",
                'ş' => "s",
                'ğ' => "g",
                'ü' => "u",
                'ö' => "o",
                'ç' => "c",
                'â' => "a",
                c if c.is_ascii_alphanumeric() => {
                    out.push(c);
                    continue;
                }
                _ => "-",
            };

            out.push_str(mapped);
        }

        let collapsed = out
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-");

        Self(collapsed.chars().take(200).collect())
    }
}

impl Title {
    pub fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();

        if trimmed.is_empty() || trimmed.chars().count() > 300 {
            return Err(AppError::Invalid(
                say::TITLE_BETWEEN_ONE_THREE_HUNDRED_CHARACTERS.into(),
            ));
        }

        Ok(Self(trimmed.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_one_address_however_it_is_typed() {
        assert_eq!(
            Email::parse(" Someone@Example.Test ")
                .expect("parses")
                .as_str(),
            "someone@example.test"
        );

        for wrong in [
            "",
            "someone",
            "@example.test",
            "someone@example",
            "a b@c.test",
        ] {
            assert!(Email::parse(wrong).is_err(), "{wrong} was accepted");
        }
    }

    #[test]
    fn a_slug_refuses_what_would_read_as_two() {
        assert!(Slug::parse("a-post").is_ok());

        for wrong in [
            "",
            "-leading",
            "trailing-",
            "two--hyphens",
            "Upper Case",
            "sl/ash",
        ] {
            assert!(Slug::parse(wrong).is_err(), "{wrong} was accepted");
        }
    }

    #[test]
    fn a_turkish_title_does_not_slug_to_nothing() {
        assert_eq!(
            Slug::from_title("Çiğdem'in Şu Işığı").as_str(),
            "cigdem-in-su-isigi"
        );
        assert_eq!(Slug::from_title("  ").as_str(), "");
    }
}
