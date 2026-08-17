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
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Api, Endpoint, Method, Permission};
use mavi_core::{
    Action, ApiKeyId, Caller, Capability, Grant, Grants, MaviError, PersonId, Result, RoleId,
    SessionId, SiteContext,
};
use mavi_storage::SiteTx;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use sqlx::Row;

pub const SETUP_ALREADY_COMPLETE: &str = "setup_already_complete";
pub const EMAIL_INVALID: &str = "email_invalid";
pub const PERSON_NAME_INVALID: &str = "person_name_invalid";
pub const PASSWORD_INVALID: &str = "password_invalid";
pub const API_KEY_NAME_INVALID: &str = "api_key_name_invalid";
pub const API_KEY_GRANTS_INVALID: &str = "api_key_grants_invalid";

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
        .takes("SetupInput")
        .returns(201, "Person"),
        Endpoint::new(
            Method::Post,
            "/api/v1/auth/sessions",
            "auth.session.create",
            "Create an account session",
        )
        .public_mutation()
        .takes("LoginInput")
        .returns(201, "Session"),
        Endpoint::new(
            Method::Delete,
            "/api/v1/auth/sessions/current",
            "auth.session.revoke",
            "Revoke the current session",
        )
        .returns(204, "Empty")
        .self_only(),
        Endpoint::new(
            Method::Post,
            "/api/v1/auth/api-keys",
            "auth.api_key.create",
            "Create an assistant API key",
        )
        .requires(Permission {
            capability: Capability::People,
            action: Action::Write,
        })
        .takes("CreateApiKey")
        .returns(201, "ApiKeyCreated")
        .changes(false),
        Endpoint::new(
            Method::Delete,
            "/api/v1/auth/api-keys/{id}",
            "auth.api_key.revoke",
            "Revoke an assistant API key",
        )
        .requires(Permission {
            capability: Capability::People,
            action: Action::Delete,
        })
        .returns(204, "Empty")
        .changes(false),
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

#[derive(Clone, Deserialize)]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

impl fmt::Debug for LoginInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoginInput")
            .field("email", &self.email)
            .field("password", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SetupStatus {
    pub initialized: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Person {
    pub id: PersonId,
    pub site_id: mavi_core::SiteId,
    pub email: Email,
    pub name: PersonName,
}

#[derive(Clone, Debug, Serialize)]
pub struct SessionCreated {
    pub id: SessionId,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateApiKey {
    pub name: String,
    pub grants: Vec<Grant>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ApiKeyCreated {
    pub id: ApiKeyId,
    pub name: String,
    pub token: String,
    pub grants: Grants,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct IdentityService;

impl IdentityService {
    pub async fn status(&self, tx: &mut SiteTx, context: &SiteContext) -> Result<SetupStatus> {
        let initialized: bool =
            sqlx::query_scalar("select exists(select 1 from people where site_id = $1)")
                .bind(context.site_id.into_uuid())
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
        Ok(SetupStatus { initialized })
    }

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

    pub async fn create_session(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &LoginInput,
        now: DateTime<Utc>,
    ) -> Result<SessionCreated> {
        let email = Email::parse(&input.email)?;
        let row = sqlx::query(
            "select id, password_hash from people
              where site_id = $1 and email = $2 and status = 'active'",
        )
        .bind(context.site_id.into_uuid())
        .bind(email.as_str())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::Unauthenticated)?;

        let password_hash: String = row
            .try_get("password_hash")
            .map_err(|_| MaviError::Internal)?;
        if !self.verify_password(&input.password, &password_hash) {
            return Err(MaviError::Unauthenticated);
        }

        let person_id = PersonId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
        let session_id = SessionId::new();
        let token = new_token();
        let expires_at = now + Duration::days(30);
        sqlx::query(
            "insert into sessions (site_id, id, person_id, token_hash, expires_at)
             values ($1, $2, $3, $4, $5)",
        )
        .bind(context.site_id.into_uuid())
        .bind(session_id.into_uuid())
        .bind(person_id.into_uuid())
        .bind(hash_token(&token))
        .bind(expires_at)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        Ok(SessionCreated {
            id: session_id,
            token,
            expires_at,
        })
    }

    pub async fn authenticate_bearer(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        token: &str,
        now: DateTime<Utc>,
    ) -> Result<Caller> {
        let row = sqlx::query(
            "select s.id, s.person_id from sessions s
               join people p on p.site_id = s.site_id and p.id = s.person_id
              where s.site_id = $1 and s.token_hash = $2
                and s.expires_at > $3 and s.revoked_at is null and p.status = 'active'",
        )
        .bind(context.site_id.into_uuid())
        .bind(hash_token(token))
        .bind(now)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        if let Some(row) = row {
            let session_id =
                SessionId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
            let person_id =
                PersonId::from_uuid(row.try_get("person_id").map_err(|_| MaviError::Internal)?);
            let grants = grants_for_person(tx, context.site_id, person_id).await?;
            return Ok(Caller::Account {
                person_id,
                session_id: Some(session_id),
                grants,
            });
        }

        let Some(prefix) = api_key_prefix(token) else {
            return Err(MaviError::Unauthenticated);
        };
        let row = sqlx::query(
            "select k.id, k.person_id from api_keys k
               join people p on p.site_id = k.site_id and p.id = k.person_id
              where k.site_id = $1 and k.prefix = $2 and k.secret_hash = $3
                and (k.expires_at is null or k.expires_at > $4)
                and k.revoked_at is null and p.status = 'active'",
        )
        .bind(context.site_id.into_uuid())
        .bind(prefix)
        .bind(hash_token(token))
        .bind(now)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::Unauthenticated)?;

        let key_id = ApiKeyId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
        let person_id =
            PersonId::from_uuid(row.try_get("person_id").map_err(|_| MaviError::Internal)?);
        let grants = grants_for_api_key(tx, context.site_id, key_id).await?;
        Ok(Caller::Assistant {
            key_id,
            person_id: Some(person_id),
            grants,
        })
    }

    pub async fn revoke_current(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let Caller::Account {
            session_id: Some(session_id),
            ..
        } = &context.caller
        else {
            return Err(MaviError::Unauthenticated);
        };
        sqlx::query(
            "update sessions set revoked_at = $3
               where site_id = $1 and id = $2 and revoked_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(session_id.into_uuid())
        .bind(now)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        Ok(())
    }

    pub async fn create_api_key(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateApiKey,
        now: DateTime<Utc>,
    ) -> Result<ApiKeyCreated> {
        let Caller::Account {
            person_id, grants, ..
        } = &context.caller
        else {
            return Err(MaviError::Forbidden);
        };
        let name = input.name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err(MaviError::validation(API_KEY_NAME_INVALID));
        }
        if input.grants.is_empty() {
            return Err(MaviError::validation(API_KEY_GRANTS_INVALID));
        }
        let mut requested = Vec::new();
        for grant in &input.grants {
            if !grants.allows(*grant) {
                return Err(MaviError::Forbidden);
            }
            if !requested.contains(grant) {
                requested.push(*grant);
            }
        }
        if input.expires_at.is_some_and(|expires_at| expires_at <= now) {
            return Err(MaviError::validation("api_key_expiry_invalid"));
        }

        let api_key_id = ApiKeyId::new();
        let token = new_prefixed_token();
        let prefix = api_key_prefix(&token).ok_or(MaviError::Internal)?;
        sqlx::query(
            "insert into api_keys (site_id, id, person_id, name, prefix, secret_hash, expires_at)
             values ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(context.site_id.into_uuid())
        .bind(api_key_id.into_uuid())
        .bind(person_id.into_uuid())
        .bind(name)
        .bind(prefix)
        .bind(hash_token(&token))
        .bind(input.expires_at)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        for grant in &requested {
            sqlx::query(
                "insert into api_key_grants (site_id, key_id, capability, action)
                 values ($1, $2, $3, $4)",
            )
            .bind(context.site_id.into_uuid())
            .bind(api_key_id.into_uuid())
            .bind(grant.capability.as_str())
            .bind(grant.action.as_str())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "auth.api_key.created".to_owned(),
                    resource_type: "ApiKey".to_owned(),
                    resource_id: Some(api_key_id.into_uuid()),
                    payload: serde_json::json!({"grant_count": requested.len()}),
                },
            )
            .await?;

        Ok(ApiKeyCreated {
            id: api_key_id,
            name: name.to_owned(),
            token,
            grants: Grants::new(requested),
            expires_at: input.expires_at,
        })
    }

    pub async fn revoke_api_key(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        key_id: ApiKeyId,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let affected = sqlx::query(
            "update api_keys set revoked_at = $3
               where site_id = $1 and id = $2 and revoked_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(key_id.into_uuid())
        .bind(now)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .rows_affected();
        if affected == 0 {
            return Err(MaviError::NotFound {
                resource: "api_key_not_found",
            });
        }

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "auth.api_key.revoked".to_owned(),
                    resource_type: "ApiKey".to_owned(),
                    resource_id: Some(key_id.into_uuid()),
                    payload: serde_json::json!({}),
                },
            )
            .await
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

async fn grants_for_person(
    tx: &mut SiteTx,
    site_id: mavi_core::SiteId,
    person_id: PersonId,
) -> Result<Grants> {
    let rows = sqlx::query(
        "select rg.capability, rg.action from person_roles pr
           join role_grants rg on rg.site_id = pr.site_id and rg.role_id = pr.role_id
          where pr.site_id = $1 and pr.person_id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(person_id.into_uuid())
    .fetch_all(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;

    let mut grants = Vec::with_capacity(rows.len());
    for row in rows {
        let capability: String = row.try_get("capability").map_err(|_| MaviError::Internal)?;
        let action: String = row.try_get("action").map_err(|_| MaviError::Internal)?;
        let capability = Capability::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == capability)
            .ok_or(MaviError::Internal)?;
        let action = Action::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == action)
            .ok_or(MaviError::Internal)?;
        grants.push(Grant::new(capability, action));
    }
    Ok(Grants::new(grants))
}

async fn grants_for_api_key(
    tx: &mut SiteTx,
    site_id: mavi_core::SiteId,
    key_id: ApiKeyId,
) -> Result<Grants> {
    let rows = sqlx::query(
        "select capability, action from api_key_grants
          where site_id = $1 and key_id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(key_id.into_uuid())
    .fetch_all(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;

    let mut grants = Vec::with_capacity(rows.len());
    for row in rows {
        let capability: String = row.try_get("capability").map_err(|_| MaviError::Internal)?;
        let action: String = row.try_get("action").map_err(|_| MaviError::Internal)?;
        let capability = Capability::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == capability)
            .ok_or(MaviError::Internal)?;
        let action = Action::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == action)
            .ok_or(MaviError::Internal)?;
        grants.push(Grant::new(capability, action));
    }
    Ok(Grants::new(grants))
}

fn new_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn new_prefixed_token() -> String {
    format!("mavi_key_{}", new_token())
}

fn api_key_prefix(token: &str) -> Option<&str> {
    token.strip_prefix("mavi_key_")?;
    token.get(..16)
}

fn hash_token(token: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};

    Sha256::digest(token.as_bytes()).to_vec()
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
    fn api_key_tokens_are_prefixed_and_prefix_lookup_rejects_sessions() {
        let token = new_prefixed_token();
        assert!(token.starts_with("mavi_key_"));
        assert_eq!(api_key_prefix(&token), Some(&token[..16]));
        assert!(api_key_prefix(&new_token()).is_none());
    }

    #[test]
    fn api_key_prefix_is_not_a_secret() {
        let token = new_prefixed_token();
        let prefix = api_key_prefix(&token).expect("prefix");
        assert_ne!(prefix, token);
        assert!(token.starts_with(prefix));
    }

    #[test]
    fn identity_api_is_valid() {
        api().validate().expect("identity API contract is valid");
    }
}
