//! Setup and account identity.
//!
//! Passwords are accepted only at this boundary and are immediately converted
//! into an Argon2id PHC digest. The rest of the application deals with a
//! `Person` and a `SiteContext`, never with a plaintext password.

use std::fmt;

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use mavi_contract::{Api, Endpoint, Method};
use mavi_core::{Action, Capability, MaviError, PersonId, Result, RoleId, SiteContext};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};

pub const SETUP_ALREADY_COMPLETE: &str = "setup_already_complete";
pub const EMAIL_INVALID: &str = "email_invalid";
pub const PERSON_NAME_INVALID: &str = "person_name_invalid";
pub const PASSWORD_INVALID: &str = "password_invalid";

/// Setup and authentication routes are public by design, but every mutation
/// is explicitly marked in the canonical contract.
#[must_use]
pub fn api() -> Api {
    Api::new([
        Endpoint::new(
            Method::Get,
            "/api/v1/setup",
            "setup.status",
            "Read setup status",
        )
        .public()
        .returns(200, "SetupStatus"),
        Endpoint::new(
            Method::Post,
            "/api/v1/setup",
            "setup.initialize",
            "Initialize the site",
        )
        .public_mutation()
        .returns(201, "Person"),
        Endpoint::new(
            Method::Post,
            "/api/v1/auth/sessions",
            "auth.session.create",
            "Create an account session",
        )
        .public_mutation()
        .returns(201, "Session"),
        Endpoint::new(
            Method::Delete,
            "/api/v1/auth/sessions/current",
            "auth.session.revoke",
            "Revoke the current session",
        )
        .returns(204, "Empty")
        .self_only(),
    ])
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Email(String);

impl Email {
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim().to_ascii_lowercase();
        let Some((local, domain)) = value.split_once('@') else {
            return Err(MaviError::validation(EMAIL_INVALID));
        };
        let valid = value.len() <= 254
            && !local.is_empty()
            && local.len() <= 64
            && !domain.is_empty()
            && domain.contains('.')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
            && !value.chars().any(char::is_whitespace);

        if !valid {
            return Err(MaviError::validation(EMAIL_INVALID));
        }

        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct PersonName(String);

impl PersonName {
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.is_empty() || value.chars().count() > 120 {
            return Err(MaviError::validation(PERSON_NAME_INVALID));
        }

        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub struct Password(String);

impl fmt::Debug for Password {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Password")
            .field("value", &"<redacted>")
            .finish()
    }
}

impl Password {
    pub fn parse(value: String) -> Result<Self> {
        let length = value.chars().count();
        if !(12..=1024).contains(&length) {
            return Err(MaviError::validation(PASSWORD_INVALID));
        }

        Ok(Self(value))
    }
}

#[derive(Clone)]
struct PasswordDigest(String);

impl PasswordDigest {
    fn from_password(password: &Password) -> Result<Self> {
        Argon2::default()
            .hash_password(password.0.as_bytes())
            .map(|hash| Self(hash.to_string()))
            .map_err(|_| MaviError::Internal)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct SetupInput {
    pub site_name: String,
    pub email: String,
    pub name: String,
    pub password: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Person {
    pub id: PersonId,
    pub site_id: mavi_core::SiteId,
    pub email: Email,
    pub name: PersonName,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct IdentityService;

impl IdentityService {
    /// Creates the first site owner exactly once. It is intentionally scoped
    /// to a public setup context: an account, assistant or operator cannot
    /// silently bootstrap another person through this method.
    pub async fn initialize(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &SetupInput,
    ) -> Result<Person> {
        if !context.caller.is_public() {
            return Err(MaviError::Forbidden);
        }

        let name = input.site_name.trim();
        if name.is_empty() || name.chars().count() > 200 {
            return Err(MaviError::validation("site_name_invalid"));
        }
        let email = Email::parse(&input.email)?;
        let person_name = PersonName::parse(&input.name)?;
        let password = Password::parse(input.password.clone())?;
        let digest = PasswordDigest::from_password(&password)?;

        let already_initialized: bool =
            sqlx::query_scalar("select exists(select 1 from people where site_id = $1)")
                .bind(context.site_id.into_uuid())
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
        if already_initialized {
            return Err(MaviError::conflict(SETUP_ALREADY_COMPLETE));
        }

        sqlx::query(
            "insert into site_settings (site_id, name) values ($1, $2)
             on conflict (site_id) do update set name = excluded.name, updated_at = now()",
        )
        .bind(context.site_id.into_uuid())
        .bind(name)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        let person_id = PersonId::new();
        let role_id = RoleId::new();
        sqlx::query("insert into roles (site_id, id, name) values ($1, $2, 'owner')")
            .bind(context.site_id.into_uuid())
            .bind(role_id.into_uuid())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;

        sqlx::query(
            "insert into people (site_id, id, email, name, password_hash)
             values ($1, $2, $3, $4, $5)",
        )
        .bind(context.site_id.into_uuid())
        .bind(person_id.into_uuid())
        .bind(email.as_str())
        .bind(person_name.as_str())
        .bind(&digest.0)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        for capability in Capability::ALL {
            for action in Action::ALL {
                sqlx::query(
                    "insert into role_grants (site_id, role_id, capability, action)
                     values ($1, $2, $3, $4)",
                )
                .bind(context.site_id.into_uuid())
                .bind(role_id.into_uuid())
                .bind(capability.as_str())
                .bind(action.as_str())
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
            }
        }

        sqlx::query("insert into person_roles (site_id, person_id, role_id) values ($1, $2, $3)")
            .bind(context.site_id.into_uuid())
            .bind(person_id.into_uuid())
            .bind(role_id.into_uuid())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;

        Ok(Person {
            id: person_id,
            site_id: context.site_id,
            email,
            name: person_name,
        })
    }

    #[must_use]
    pub fn verify_password(&self, password: &str, digest: &str) -> bool {
        let Ok(parsed) = PasswordHash::new(digest) else {
            return false;
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_is_normalized_before_storage() {
        let email = Email::parse("  Owner@Example.COM ").expect("valid email");
        assert_eq!(email.as_str(), "owner@example.com");
        assert!(Email::parse("not-an-email").is_err());
    }

    #[test]
    fn password_policy_rejects_short_values() {
        assert!(Password::parse("too-short".to_owned()).is_err());
        assert!(Password::parse("long-enough-password".to_owned()).is_ok());
    }

    #[test]
    fn argon2_digest_verifies_only_the_original_password() {
        let service = IdentityService;
        let password = Password::parse("long-enough-password".to_owned()).expect("password");
        let digest = PasswordDigest::from_password(&password).expect("digest");

        assert!(service.verify_password("long-enough-password", &digest.0));
        assert!(!service.verify_password("different-password", &digest.0));
    }

    #[test]
    fn identity_api_is_valid() {
        api().validate().expect("identity API contract is valid");
    }
}
