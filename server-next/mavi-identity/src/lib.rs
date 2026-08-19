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
use mavi_contract::{Api, Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, ApiKeyId, Caller, Capability, Cursor, EmailVerificationTokenId, ErrorCode, Grant,
    Grants, MaviError, Page, PageRequest, PasswordResetTokenId, PersonId, Result, RoleId,
    SessionId, SiteContext, SiteId,
};
use mavi_storage::SiteTx;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

pub const SETUP_ALREADY_COMPLETE: &str = "setup_already_complete";
pub const EMAIL_INVALID: &str = "email_invalid";
pub const PERSON_NAME_INVALID: &str = "person_name_invalid";
pub const PASSWORD_INVALID: &str = "password_invalid";
pub const API_KEY_NAME_INVALID: &str = "api_key_name_invalid";
pub const API_KEY_GRANTS_INVALID: &str = "api_key_grants_invalid";
pub const API_KEY_NOT_FOUND: &str = "api_key_not_found";
pub const PASSWORD_RESET_TOKEN_INVALID: &str = "password_reset_token_invalid";
pub const EMAIL_VERIFICATION_TOKEN_INVALID: &str = "email_verification_token_invalid";
pub const EMAIL_NOT_VERIFIED: &str = "email_not_verified";
pub const SITE_NOT_FOUND: &str = "site_not_found";
pub const PERSON_NOT_FOUND: &str = "person_not_found";
pub const ROLE_NAME_INVALID: &str = "role_name_invalid";
pub const ROLE_NOT_FOUND: &str = "role_not_found";
pub const ROLE_ASSIGNED: &str = "role_assigned";
pub const OWNER_ROLE_PROTECTED: &str = "owner_role_protected";

/// Stable audit action names for account-security events.
pub mod audit_action {
    pub const SETUP_INITIALIZED: &str = "auth.setup.initialized";
    pub const SESSION_FAILED: &str = "auth.session.failed";
    pub const SESSION_BLOCKED: &str = "auth.session.blocked";
    pub const SESSION_CREATED: &str = "auth.session.created";
    pub const SESSION_REVOKED: &str = "auth.session.revoked";
    pub const PASSWORD_RESET_REQUESTED: &str = "auth.password_reset.requested";
    pub const PASSWORD_RESET_REDEEMED: &str = "auth.password_reset.redeemed";
    pub const EMAIL_VERIFICATION_REQUESTED: &str = "auth.email_verification.requested";
    pub const EMAIL_VERIFICATION_REDEEMED: &str = "auth.email_verification.redeemed";
    pub const SECURITY_SUBJECT_RATE_LIMITED: &str = "auth.security.subject_rate_limited";
    pub const SECURITY_EDGE_RATE_LIMITED: &str = "auth.security.edge_rate_limited";
    pub const API_KEY_CREATED: &str = "auth.api_key.created";
    pub const API_KEY_REVOKED: &str = "auth.api_key.revoked";
}
const PASSWORD_RESET_TTL: Duration = Duration::hours(1);
const MAX_PASSWORD_RESET_TOKEN_CHARS: usize = 256;
const EMAIL_VERIFICATION_TTL: Duration = Duration::hours(24);
const MAX_EMAIL_VERIFICATION_TOKEN_CHARS: usize = 256;
const AUTH_REQUEST_WINDOW: Duration = Duration::hours(1);
const MAX_AUTH_REQUESTS_PER_WINDOW: i32 = 5;

/// Setup and authentication routes are public by design, but every mutation
/// is explicitly marked in the canonical contract.
#[allow(clippy::too_many_lines)]
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
        .returns(201, "SessionCreated")
        .refuses([
            ErrorCode::Validation,
            ErrorCode::Unauthenticated,
            ErrorCode::Conflict,
            ErrorCode::RateLimited,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/auth/password-resets",
            "auth.password_reset.request",
            "Request a password reset without revealing account existence",
        )
        .public_mutation()
        .takes("PasswordResetRequest")
        .returns(202, "PasswordResetRequested")
        .refuses([
            ErrorCode::Validation,
            ErrorCode::RateLimited,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/auth/password-resets/redeem",
            "auth.password_reset.redeem",
            "Redeem a one-time password reset token",
        )
        .public_changes(false)
        .takes("PasswordResetRedeem")
        .returns(204, "Empty")
        .refuses([
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::RateLimited,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/auth/email-verifications",
            "auth.email_verification.request",
            "Request email verification without revealing account existence",
        )
        .public_mutation()
        .takes("EmailVerificationRequest")
        .returns(202, "EmailVerificationRequested")
        .refuses([
            ErrorCode::Validation,
            ErrorCode::RateLimited,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/auth/email-verifications/redeem",
            "auth.email_verification.redeem",
            "Redeem a one-time email verification token",
        )
        .public_changes(false)
        .takes("EmailVerificationRedeem")
        .returns(204, "Empty")
        .refuses([
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::RateLimited,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/auth/sessions/current",
            "auth.session.revoke",
            "Revoke the current session",
        )
        .returns(204, "Empty")
        .self_only(),
        Endpoint::new(
            Method::Get,
            "/api/v1/auth/sessions/current",
            "auth.session.current",
            "Read the current account session",
        )
        .returns(200, "CurrentSession")
        .self_only()
        .refuses([
            ErrorCode::Unauthenticated,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/auth/api-keys",
            "auth.api_key.list",
            "List assistant API key metadata",
        )
        .requires(Permission {
            capability: Capability::People,
            action: Action::View,
        })
        .takes_query("ApiKeyListFilter")
        .returns(200, "ApiKeyPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
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
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/auth/api-keys/{id}",
            "auth.api_key.revoke",
            "Revoke an assistant API key",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::People,
            action: Action::Delete,
        })
        .returns(204, "Empty")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/people",
            "people.list",
            "List site people",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::People,
            action: Action::View,
        })
        .takes_query("PeopleListFilter")
        .returns(200, "PersonPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/people",
            "people.create",
            "Create a site person",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::People,
            action: Action::Write,
        })
        .takes("CreatePerson")
        .returns(201, "PersonRecord")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/people/{id}/status",
            "people.status.update",
            "Update a person's status",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::People,
            action: Action::Write,
        })
        .takes("UpdatePersonStatus")
        .returns(200, "PersonRecord")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Put,
            "/api/v1/people/{id}/roles",
            "people.roles.replace",
            "Replace a person's roles",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::People,
            action: Action::Write,
        })
        .takes("ReplacePersonRoles")
        .returns(200, "PersonRecord")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/roles",
            "roles.list",
            "List site roles",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::People,
            action: Action::View,
        })
        .takes_query("RoleListFilter")
        .returns(200, "RolePage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/roles",
            "roles.create",
            "Create a site role",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::People,
            action: Action::Write,
        })
        .takes("CreateRole")
        .returns(201, "Role")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/roles/{id}",
            "roles.delete",
            "Delete an unassigned site role",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::People,
            action: Action::Delete,
        })
        .returns(204, "Empty")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Put,
            "/api/v1/roles/{id}/grants",
            "roles.grants.replace",
            "Replace a role grants set",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::People,
            action: Action::Write,
        })
        .takes("ReplaceRoleGrants")
        .returns(200, "Role")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
    ])
    .with_shapes(identity_shapes())
}

#[allow(clippy::too_many_lines)]
fn identity_shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "SetupStatus",
            json!({
                "type": "object",
                "required": ["initialized"],
                "properties": {"initialized": {"type": "boolean"}},
            }),
        ),
        Shape::new(
            "SetupInput",
            json!({
                "type": "object",
                "required": ["site_name", "email", "name", "password"],
                "properties": {
                    "site_name": {"type": "string", "maxLength": 200},
                    "email": {"type": "string", "format": "email"},
                    "name": {"type": "string", "maxLength": 120},
                    "password": {"type": "string", "format": "password", "minLength": 12},
                },
            }),
        ),
        Shape::new(
            "LoginInput",
            json!({
                "type": "object",
                "required": ["email", "password"],
                "properties": {
                    "email": {"type": "string", "format": "email"},
                    "password": {"type": "string", "format": "password"},
                },
            }),
        ),
        Shape::new(
            "PasswordResetRequest",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["email"],
                "properties": {"email": {"type": "string", "format": "email"}},
            }),
        ),
        Shape::new(
            "PasswordResetRedeem",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["token", "password"],
                "properties": {
                    "token": {"type": "string", "minLength": 1, "maxLength": MAX_PASSWORD_RESET_TOKEN_CHARS},
                    "password": {"type": "string", "format": "password", "minLength": 12},
                },
            }),
        ),
        Shape::new(
            "PasswordResetRequested",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["accepted"],
                "properties": {"accepted": {"type": "boolean", "const": true}},
            }),
        ),
        Shape::new(
            "EmailVerificationRequest",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["email"],
                "properties": {"email": {"type": "string", "format": "email"}},
            }),
        ),
        Shape::new(
            "EmailVerificationRedeem",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["token"],
                "properties": {
                    "token": {"type": "string", "minLength": 1, "maxLength": MAX_EMAIL_VERIFICATION_TOKEN_CHARS},
                },
            }),
        ),
        Shape::new(
            "EmailVerificationRequested",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["accepted"],
                "properties": {"accepted": {"type": "boolean", "const": true}},
            }),
        ),
        Shape::new(
            "Person",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "site_id", "email", "name", "email_verified"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "site_id": {"type": "string", "format": "uuid"},
                    "email": {"type": "string", "format": "email"},
                    "name": {"type": "string"},
                    "email_verified": {"type": "boolean"},
                },
            }),
        ),
        Shape::new(
            "SessionCreated",
            json!({
                "type": "object",
                "required": ["id", "token", "expires_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "token": {"type": "string"},
                    "expires_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "CurrentSession",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["person", "grants"],
                "properties": {
                    "person": {"$ref": "#/components/schemas/PersonRecord"},
                    "grants": {"type": "array", "items": {"$ref": "#/components/schemas/Grant"}},
                },
            }),
        ),
        Shape::new(
            "Grant",
            json!({
                "type": "object",
                "required": ["capability", "action"],
                "properties": {
                    "capability": {"type": "string"},
                    "action": {"type": "string"},
                },
            }),
        ),
        Shape::new(
            "CreateApiKey",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "grants"],
                "properties": {
                    "name": {"type": "string", "maxLength": 120},
                    "grants": {"type": "array", "items": {"$ref": "#/components/schemas/Grant"}},
                    "expires_at": {"type": ["string", "null"], "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "ApiKeyCreated",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "site_id", "person_id", "name", "prefix", "token", "grants", "expires_at", "created_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "site_id": {"type": "string", "format": "uuid"},
                    "person_id": {"type": "string", "format": "uuid"},
                    "name": {"type": "string"},
                    "prefix": {"type": "string", "maxLength": 16},
                    "token": {"type": "string"},
                    "grants": {"type": "array", "items": {"$ref": "#/components/schemas/Grant"}},
                    "expires_at": {"type": ["string", "null"], "format": "date-time"},
                    "created_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "ApiKeyListFilter",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                    "revoked": {"type": ["boolean", "null"]},
                },
            }),
        ),
        Shape::new(
            "ApiKeyRecord",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "site_id", "person_id", "name", "prefix", "grants", "expires_at", "revoked_at", "created_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "site_id": {"type": "string", "format": "uuid"},
                    "person_id": {"type": "string", "format": "uuid"},
                    "name": {"type": "string"},
                    "prefix": {"type": "string", "maxLength": 16},
                    "grants": {"type": "array", "items": {"$ref": "#/components/schemas/Grant"}},
                    "expires_at": {"type": ["string", "null"], "format": "date-time"},
                    "revoked_at": {"type": ["string", "null"], "format": "date-time"},
                    "created_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "ApiKeyPage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/ApiKeyRecord"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512},
                },
            }),
        ),
        Shape::new(
            "PersonListFilterStatus",
            json!({"type": "string", "enum": ["active", "suspended", "removed"]}),
        ),
        Shape::new(
            "PeopleListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                    "status": {"$ref": "#/components/schemas/PersonListFilterStatus"},
                },
            }),
        ),
        Shape::new(
            "CreatePerson",
            json!({
                "type": "object",
                "required": ["email", "name", "password"],
                "properties": {
                    "email": {"type": "string", "format": "email"},
                    "name": {"type": "string", "maxLength": 120},
                    "password": {"type": "string", "format": "password", "minLength": 12},
                    "role_ids": {"type": "array", "items": {"type": "string", "format": "uuid"}},
                },
            }),
        ),
        Shape::new(
            "UpdatePersonStatus",
            json!({
                "type": "object",
                "required": ["status"],
                "properties": {"status": {"$ref": "#/components/schemas/PersonListFilterStatus"}},
            }),
        ),
        Shape::new(
            "ReplacePersonRoles",
            json!({
                "type": "object",
                "required": ["role_ids"],
                "properties": {
                    "role_ids": {"type": "array", "items": {"type": "string", "format": "uuid"}},
                },
            }),
        ),
        Shape::new(
            "PersonRecord",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "site_id", "email", "name", "status", "email_verified", "role_ids", "created_at", "updated_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "site_id": {"type": "string", "format": "uuid"},
                    "email": {"type": "string", "format": "email"},
                    "name": {"type": "string"},
                    "status": {"$ref": "#/components/schemas/PersonListFilterStatus"},
                    "email_verified": {"type": "boolean"},
                    "role_ids": {"type": "array", "items": {"type": "string", "format": "uuid"}},
                    "created_at": {"type": "string", "format": "date-time"},
                    "updated_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "PersonPage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/PersonRecord"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512},
                },
            }),
        ),
        Shape::new(
            "RoleListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                },
            }),
        ),
        Shape::new(
            "Role",
            json!({
                "type": "object",
                "required": ["id", "site_id", "name", "grants", "created_at", "protected"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "site_id": {"type": "string", "format": "uuid"},
                    "name": {"type": "string"},
                    "grants": {"type": "array", "items": {"$ref": "#/components/schemas/Grant"}},
                    "created_at": {"type": "string", "format": "date-time"},
                    "protected": {"type": "boolean"},
                },
            }),
        ),
        Shape::new(
            "RolePage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/Role"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512},
                },
            }),
        ),
        Shape::new(
            "CreateRole",
            json!({
                "type": "object",
                "required": ["name"],
                "properties": {
                    "name": {"type": "string", "maxLength": 64},
                    "grants": {"type": "array", "items": {"$ref": "#/components/schemas/Grant"}},
                },
            }),
        ),
        Shape::new(
            "ReplaceRoleGrants",
            json!({
                "type": "object",
                "required": ["grants"],
                "properties": {
                    "grants": {"type": "array", "items": {"$ref": "#/components/schemas/Grant"}},
                },
            }),
        ),
    ]
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Email(String);

impl Email {
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim().to_ascii_lowercase();
        let mut parts = value.split('@');
        let Some(local) = parts.next() else {
            return Err(MaviError::validation(EMAIL_INVALID));
        };
        let Some(domain) = parts.next() else {
            return Err(MaviError::validation(EMAIL_INVALID));
        };
        if parts.next().is_some() {
            return Err(MaviError::validation(EMAIL_INVALID));
        }
        let valid_domain = domain.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        });
        let valid = value.len() <= 254
            && !local.is_empty()
            && local.len() <= 64
            && !domain.is_empty()
            && domain.contains('.')
            && valid_domain
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

#[derive(Clone, Deserialize)]
pub struct SetupInput {
    pub site_name: String,
    pub email: String,
    pub name: String,
    pub password: String,
}

impl fmt::Debug for SetupInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SetupInput")
            .field("site_name", &self.site_name)
            .field("email", &self.email)
            .field("name", &self.name)
            .field("password", &"<redacted>")
            .finish()
    }
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasswordResetRequestInput {
    pub email: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasswordResetRedeemInput {
    pub token: String,
    pub password: String,
}

impl fmt::Debug for PasswordResetRedeemInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PasswordResetRedeemInput")
            .field("token", &"<redacted>")
            .field("password", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmailVerificationRequestInput {
    pub email: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmailVerificationRedeemInput {
    pub token: String,
}

impl fmt::Debug for EmailVerificationRedeemInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmailVerificationRedeemInput")
            .field("token", &"<redacted>")
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
    pub email_verified: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonStatus {
    Active,
    Suspended,
    Removed,
}

impl PersonStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Removed => "removed",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "active" => Ok(Self::Active),
            "suspended" => Ok(Self::Suspended),
            "removed" => Ok(Self::Removed),
            _ => Err(MaviError::Internal),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PersonRecord {
    pub id: PersonId,
    pub site_id: SiteId,
    pub email: Email,
    pub name: PersonName,
    pub status: PersonStatus,
    pub email_verified: bool,
    pub role_ids: Vec<RoleId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreatePerson {
    pub email: String,
    pub name: String,
    pub password: String,
    #[serde(default)]
    pub role_ids: Vec<RoleId>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpdatePersonStatus {
    pub status: PersonStatus,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ReplacePersonRoles {
    pub role_ids: Vec<RoleId>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PeopleListFilter {
    pub status: Option<PersonStatus>,
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RoleListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RoleName(String);

impl RoleName {
    pub fn parse(value: &str) -> Result<Self> {
        let valid = !value.is_empty()
            && value.len() <= 64
            && value.starts_with(|character: char| character.is_ascii_lowercase())
            && value.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || character == '_'
                    || character == '-'
            });
        if !valid {
            return Err(MaviError::validation(ROLE_NAME_INVALID));
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Role {
    pub id: RoleId,
    pub site_id: SiteId,
    pub name: RoleName,
    pub grants: Grants,
    pub created_at: DateTime<Utc>,
    pub protected: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateRole {
    pub name: String,
    #[serde(default)]
    pub grants: Vec<Grant>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ReplaceRoleGrants {
    pub grants: Vec<Grant>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SessionCreated {
    pub id: SessionId,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CurrentSession {
    pub person: PersonRecord,
    pub grants: Grants,
}

#[derive(Clone, Debug, Serialize)]
pub struct PasswordResetRequested {
    pub accepted: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct EmailVerificationRequested {
    pub accepted: bool,
}

/// Internal application output used by the HTTP composition root to enqueue
/// a provider-neutral notification in the same site transaction. The raw
/// token never crosses an API response or an audit record.
#[derive(Clone)]
pub struct PasswordResetNotification {
    pub id: PasswordResetTokenId,
    pub recipient: Email,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

impl fmt::Debug for PasswordResetNotification {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PasswordResetNotification")
            .field("id", &self.id)
            .field("recipient", &self.recipient)
            .field("token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Internal application output used by the HTTP composition root to enqueue
/// a provider-neutral verification notification in the same transaction.
/// The raw token never crosses an API response or an audit record.
#[derive(Clone)]
pub struct EmailVerificationNotification {
    pub id: EmailVerificationTokenId,
    pub recipient: Email,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

impl fmt::Debug for EmailVerificationNotification {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmailVerificationNotification")
            .field("id", &self.id)
            .field("recipient", &self.recipient)
            .field("token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateApiKey {
    pub name: String,
    pub grants: Vec<Grant>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApiKeyListFilter {
    pub revoked: Option<bool>,
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Serialize)]
pub struct ApiKeyCreated {
    pub id: ApiKeyId,
    pub site_id: SiteId,
    pub person_id: PersonId,
    pub name: String,
    pub prefix: String,
    pub token: String,
    pub grants: Grants,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ApiKeyRecord {
    pub id: ApiKeyId,
    pub site_id: SiteId,
    pub person_id: PersonId,
    pub name: String,
    pub prefix: String,
    pub grants: Grants,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
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

    /// Returns the normalized email of the site's owner, when setup has
    /// already completed. The operator uses this only to make a retried
    /// provisioning command idempotent; it does not expose the password or
    /// grant set and it does not change the public setup response shape.
    pub async fn owner_email(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
    ) -> Result<Option<String>> {
        sqlx::query_scalar(
            "select p.email
               from people p
               join person_roles pr on pr.site_id = p.site_id and pr.person_id = p.id
               join roles r on r.site_id = pr.site_id and r.id = pr.role_id
              where p.site_id = $1 and r.name = 'owner'
              order by p.created_at asc
              limit 1",
        )
        .bind(context.site_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)
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

        lock_site(tx, context.site_id).await?;

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

        let person_id = PersonId::new();
        let role_id = RoleId::new();
        sqlx::query(
            "insert into roles (site_id, id, name, system_role)
             values ($1, $2, 'owner', true)",
        )
        .bind(context.site_id.into_uuid())
        .bind(role_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        sqlx::query(
            "insert into people
                (site_id, id, email, name, password_hash, email_verified_at)
             values ($1, $2, $3, $4, $5, now())",
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

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: audit_action::SETUP_INITIALIZED.to_owned(),
                    resource_type: "Site".to_owned(),
                    resource_id: Some(context.site_id.into_uuid()),
                    payload: serde_json::json!({"owner_person_id": person_id}),
                },
            )
            .await?;

        Ok(Person {
            id: person_id,
            site_id: context.site_id,
            email,
            name: person_name,
            email_verified: true,
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
        let email_hash = URL_SAFE_NO_PAD.encode(hash_token(email.as_str()));
        let Some(row) = sqlx::query(
            "select id, password_hash, email_verified_at from people
              where site_id = $1 and email = $2 and status = 'active'",
        )
        .bind(context.site_id.into_uuid())
        .bind(email.as_str())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        else {
            record_auth_audit(
                tx,
                context,
                audit_action::SESSION_FAILED,
                "Site",
                Some(context.site_id.into_uuid()),
                json!({"outcome": "invalid_credentials", "email_hash": email_hash}),
            )
            .await?;
            return Err(MaviError::Unauthenticated);
        };

        let password_hash: String = row
            .try_get("password_hash")
            .map_err(|_| MaviError::Internal)?;
        if !self.verify_password(&input.password, &password_hash) {
            record_auth_audit(
                tx,
                context,
                audit_action::SESSION_FAILED,
                "Site",
                Some(context.site_id.into_uuid()),
                json!({"outcome": "invalid_credentials", "email_hash": email_hash}),
            )
            .await?;
            return Err(MaviError::Unauthenticated);
        }

        let person_id = PersonId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
        let email_verified_at: Option<DateTime<Utc>> = row
            .try_get("email_verified_at")
            .map_err(|_| MaviError::Internal)?;
        if email_verified_at.is_none() {
            record_auth_audit(
                tx,
                context,
                audit_action::SESSION_BLOCKED,
                "Person",
                Some(person_id.into_uuid()),
                json!({"outcome": "email_unverified"}),
            )
            .await?;
            return Err(MaviError::conflict(EMAIL_NOT_VERIFIED));
        }
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

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: audit_action::SESSION_CREATED.to_owned(),
                    resource_type: "Session".to_owned(),
                    resource_id: Some(session_id.into_uuid()),
                    payload: serde_json::json!({"person_id": person_id, "expires_at": expires_at}),
                },
            )
            .await?;

        Ok(SessionCreated {
            id: session_id,
            token,
            expires_at,
        })
    }

    /// Starts a password reset without disclosing whether the address is an
    /// account in this site. When an eligible person exists, the returned
    /// notification is consumed by the application layer to enqueue mail in
    /// this same transaction.
    pub async fn request_password_reset(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &PasswordResetRequestInput,
        now: DateTime<Utc>,
    ) -> Result<Option<PasswordResetNotification>> {
        let email = Email::parse(&input.email)?;
        let email_hash = URL_SAFE_NO_PAD.encode(hash_token(email.as_str()));
        let row = sqlx::query(
            "select id, email, status from people
              where site_id = $1 and email = $2
              for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(email.as_str())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        let Some(row) = row else {
            record_auth_audit(
                tx,
                context,
                audit_action::PASSWORD_RESET_REQUESTED,
                "Site",
                Some(context.site_id.into_uuid()),
                json!({"outcome": "not_found", "email_hash": email_hash}),
            )
            .await?;
            return Ok(None);
        };

        let person_id = PersonId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
        let status: String = row.try_get("status").map_err(|_| MaviError::Internal)?;
        if status != PersonStatus::Active.as_str() {
            record_auth_audit(
                tx,
                context,
                audit_action::PASSWORD_RESET_REQUESTED,
                "Person",
                Some(person_id.into_uuid()),
                json!({"outcome": "ineligible", "email_hash": email_hash}),
            )
            .await?;
            return Ok(None);
        }

        if !allow_auth_request(
            tx,
            context.site_id,
            "password_reset",
            hash_token(email.as_str()),
            now,
        )
        .await?
        {
            record_auth_audit(
                tx,
                context,
                audit_action::SECURITY_SUBJECT_RATE_LIMITED,
                "Person",
                Some(person_id.into_uuid()),
                json!({"action": "password_reset", "email_hash": email_hash}),
            )
            .await?;
            return Ok(None);
        }

        let (token_id, token, expires_at) =
            issue_password_reset_token(tx, context.site_id, person_id, now).await?;

        record_auth_audit(
            tx,
            context,
            audit_action::PASSWORD_RESET_REQUESTED,
            "Person",
            Some(person_id.into_uuid()),
            json!({
                "outcome": "issued",
                "email_hash": email_hash,
                "expires_at": expires_at,
            }),
        )
        .await?;

        Ok(Some(PasswordResetNotification {
            id: token_id,
            recipient: Email::parse(
                row.try_get::<String, _>("email")
                    .map_err(|_| MaviError::Internal)?
                    .as_str(),
            )?,
            token,
            expires_at,
        }))
    }

    /// Starts email verification without disclosing whether the address is an
    /// active, already verified account. The token and the audit receipt are
    /// site-scoped; only the application layer receives the raw token so it
    /// can enqueue the transactional message in this transaction.
    pub async fn request_email_verification(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &EmailVerificationRequestInput,
        now: DateTime<Utc>,
    ) -> Result<Option<EmailVerificationNotification>> {
        let email = Email::parse(&input.email)?;
        let email_hash = URL_SAFE_NO_PAD.encode(hash_token(email.as_str()));
        let row = sqlx::query(
            "select id, email, status, email_verified_at from people
              where site_id = $1 and email = $2
              for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(email.as_str())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        let Some(row) = row else {
            record_auth_audit(
                tx,
                context,
                audit_action::EMAIL_VERIFICATION_REQUESTED,
                "Site",
                Some(context.site_id.into_uuid()),
                json!({"outcome": "not_found", "email_hash": email_hash}),
            )
            .await?;
            return Ok(None);
        };

        let person_id = PersonId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
        let status: String = row.try_get("status").map_err(|_| MaviError::Internal)?;
        if status != PersonStatus::Active.as_str() {
            record_auth_audit(
                tx,
                context,
                audit_action::EMAIL_VERIFICATION_REQUESTED,
                "Person",
                Some(person_id.into_uuid()),
                json!({"outcome": "ineligible", "email_hash": email_hash}),
            )
            .await?;
            return Ok(None);
        }

        let email_verified_at: Option<DateTime<Utc>> = row
            .try_get("email_verified_at")
            .map_err(|_| MaviError::Internal)?;
        if email_verified_at.is_some() {
            record_auth_audit(
                tx,
                context,
                audit_action::EMAIL_VERIFICATION_REQUESTED,
                "Person",
                Some(person_id.into_uuid()),
                json!({"outcome": "already_verified"}),
            )
            .await?;
            return Ok(None);
        }

        if !allow_auth_request(
            tx,
            context.site_id,
            "email_verification",
            hash_token(email.as_str()),
            now,
        )
        .await?
        {
            record_auth_audit(
                tx,
                context,
                audit_action::SECURITY_SUBJECT_RATE_LIMITED,
                "Person",
                Some(person_id.into_uuid()),
                json!({"action": "email_verification", "email_hash": email_hash}),
            )
            .await?;
            return Ok(None);
        }

        let (token_id, token, expires_at) =
            issue_email_verification_token(tx, context.site_id, person_id, now).await?;

        record_auth_audit(
            tx,
            context,
            audit_action::EMAIL_VERIFICATION_REQUESTED,
            "Person",
            Some(person_id.into_uuid()),
            json!({
                "outcome": "issued",
                "email_hash": email_hash,
                "expires_at": expires_at,
            }),
        )
        .await?;

        Ok(Some(EmailVerificationNotification {
            id: token_id,
            recipient: Email::parse(
                row.try_get::<String, _>("email")
                    .map_err(|_| MaviError::Internal)?
                    .as_str(),
            )?,
            token,
            expires_at,
        }))
    }

    /// Redeems one email verification token under a row lock. Verification,
    /// token consumption, token revocation and audit are one atomic operation.
    pub async fn redeem_email_verification(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &EmailVerificationRedeemInput,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let token = input.token.trim();
        if token.is_empty()
            || token.chars().count() > MAX_EMAIL_VERIFICATION_TOKEN_CHARS
            || token.chars().any(char::is_control)
            || !token.starts_with("mavi_verify_")
        {
            return Err(MaviError::conflict(EMAIL_VERIFICATION_TOKEN_INVALID));
        }

        let row = sqlx::query(
            "select id, person_id from email_verification_tokens
              where site_id = $1 and token_hash = $2 and expires_at > $3
                and used_at is null and revoked_at is null
              for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(hash_token(token))
        .bind(now)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or_else(|| MaviError::conflict(EMAIL_VERIFICATION_TOKEN_INVALID))?;
        let token_id = EmailVerificationTokenId::from_uuid(
            row.try_get("id").map_err(|_| MaviError::Internal)?,
        );
        let person_id =
            PersonId::from_uuid(row.try_get("person_id").map_err(|_| MaviError::Internal)?);

        let changed = sqlx::query(
            "update people
                set email_verified_at = $3, updated_at = $3
              where site_id = $1 and id = $2 and status = 'active'
                and email_verified_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(person_id.into_uuid())
        .bind(now)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if changed.rows_affected() != 1 {
            return Err(MaviError::conflict(EMAIL_VERIFICATION_TOKEN_INVALID));
        }

        sqlx::query(
            "update email_verification_tokens
                set used_at = $3
              where site_id = $1 and id = $2 and used_at is null and revoked_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(token_id.into_uuid())
        .bind(now)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        sqlx::query(
            "update email_verification_tokens
                set revoked_at = $3
              where site_id = $1 and person_id = $2 and id <> $4
                and used_at is null and revoked_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(person_id.into_uuid())
        .bind(now)
        .bind(token_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: audit_action::EMAIL_VERIFICATION_REDEEMED.to_owned(),
                    resource_type: "Person".to_owned(),
                    resource_id: Some(person_id.into_uuid()),
                    payload: json!({"email_verification_token_id": token_id}),
                },
            )
            .await
    }

    /// Redeems one reset token under a row lock. Password changes, session
    /// revocation, token consumption and audit are one atomic site-scoped
    /// operation, so a retry can never consume the same token twice.
    pub async fn redeem_password_reset(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &PasswordResetRedeemInput,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let password = Password::parse(input.password.clone())?;
        let token = input.token.trim();
        if token.is_empty()
            || token.chars().count() > MAX_PASSWORD_RESET_TOKEN_CHARS
            || token.chars().any(char::is_control)
        {
            return Err(MaviError::conflict(PASSWORD_RESET_TOKEN_INVALID));
        }

        let row = sqlx::query(
            "select id, person_id from password_reset_tokens
              where site_id = $1 and token_hash = $2 and expires_at > $3
                and used_at is null and revoked_at is null
              for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(hash_token(token))
        .bind(now)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or_else(|| MaviError::conflict(PASSWORD_RESET_TOKEN_INVALID))?;
        let token_id =
            PasswordResetTokenId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
        let person_id =
            PersonId::from_uuid(row.try_get("person_id").map_err(|_| MaviError::Internal)?);
        let digest = PasswordDigest::from_password(&password)?;

        let changed = sqlx::query(
            "update people
                set password_hash = $3, updated_at = $4
              where site_id = $1 and id = $2 and status = 'active'",
        )
        .bind(context.site_id.into_uuid())
        .bind(person_id.into_uuid())
        .bind(&digest.0)
        .bind(now)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if changed.rows_affected() != 1 {
            return Err(MaviError::conflict(PASSWORD_RESET_TOKEN_INVALID));
        }

        sqlx::query(
            "update password_reset_tokens
                set used_at = $3
              where site_id = $1 and id = $2 and used_at is null and revoked_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(token_id.into_uuid())
        .bind(now)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        sqlx::query(
            "update password_reset_tokens
                set revoked_at = $3
              where site_id = $1 and person_id = $2 and id <> $4
                and used_at is null and revoked_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(person_id.into_uuid())
        .bind(now)
        .bind(token_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        sqlx::query(
            "update sessions set revoked_at = $3
              where site_id = $1 and person_id = $2 and revoked_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(person_id.into_uuid())
        .bind(now)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: audit_action::PASSWORD_RESET_REDEEMED.to_owned(),
                    resource_type: "Person".to_owned(),
                    resource_id: Some(person_id.into_uuid()),
                    payload: json!({"password_reset_token_id": token_id}),
                },
            )
            .await
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

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: audit_action::SESSION_REVOKED.to_owned(),
                    resource_type: "Session".to_owned(),
                    resource_id: Some(session_id.into_uuid()),
                    payload: serde_json::json!({}),
                },
            )
            .await?;
        Ok(())
    }

    pub async fn current_session(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
    ) -> Result<CurrentSession> {
        let Caller::Account {
            person_id, grants, ..
        } = &context.caller
        else {
            return Err(MaviError::Unauthenticated);
        };

        Ok(CurrentSession {
            person: self.get_person(tx, context, *person_id).await?,
            grants: grants.clone(),
        })
    }

    async fn get_person(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        person_id: PersonId,
    ) -> Result<PersonRecord> {
        let row = sqlx::query(
            "select p.id, p.site_id, p.email, p.name, p.status, p.email_verified_at,
                    p.created_at, p.updated_at,
                    coalesce(array_agg(pr.role_id) filter (where pr.role_id is not null), '{}'::uuid[]) as role_ids
               from people p
               left join person_roles pr on pr.site_id = p.site_id and pr.person_id = p.id
              where p.site_id = $1 and p.id = $2
              group by p.id, p.site_id, p.email, p.name, p.status, p.email_verified_at,
                       p.created_at, p.updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(person_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: PERSON_NOT_FOUND,
        })?;
        person_from_row(&row)
    }

    pub async fn list_people(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &PeopleListFilter,
    ) -> Result<Page<PersonRecord>> {
        require_context_grant(context, Grant::new(Capability::People, Action::View))?;
        let after = filter
            .page
            .after
            .as_ref()
            .map(decode_identity_cursor)
            .transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select p.id, p.site_id, p.email, p.name, p.status, p.email_verified_at,
                    p.created_at, p.updated_at,
                    coalesce(array_agg(pr.role_id) filter (where pr.role_id is not null), '{}'::uuid[]) as role_ids
               from people p
               left join person_roles pr on pr.site_id = p.site_id and pr.person_id = p.id
              where p.site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        if let Some(status) = filter.status {
            query.push(" and p.status = ").push_bind(status.as_str());
        }
        if let Some(after) = after {
            query
                .push(" and (p.created_at, p.id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        let rows = query
            .push(
                " group by p.id, p.site_id, p.email, p.name, p.status, p.email_verified_at,
                            p.created_at, p.updated_at",
            )
            .push(" order by p.created_at desc, p.id desc limit ")
            .push_bind(limit + 1)
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;

        let limit_usize = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if rows.len() > limit_usize {
            let row = rows
                .get(limit_usize.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_identity_cursor(
                row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                row.try_get("id").map_err(|_| MaviError::Internal)?,
            )?)
        } else {
            None
        };
        let items = rows
            .into_iter()
            .take(limit_usize)
            .map(|row| person_from_row(&row))
            .collect::<Result<Vec<_>>>()?;
        Ok(Page::new(items, next_cursor))
    }

    pub async fn create_person(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreatePerson,
    ) -> Result<PersonRecord> {
        require_context_grant(context, Grant::new(Capability::People, Action::Write))?;
        let email = Email::parse(&input.email)?;
        let name = PersonName::parse(&input.name)?;
        let password = Password::parse(input.password.clone())?;
        let digest = PasswordDigest::from_password(&password)?;
        let role_ids = unique_role_ids(&input.role_ids);
        ensure_roles_are_delegable(tx, context, &role_ids).await?;

        let person_id = PersonId::new();
        sqlx::query(
            "insert into people
                (site_id, id, email, name, password_hash, email_verified_at)
             values ($1, $2, $3, $4, $5, null)",
        )
        .bind(context.site_id.into_uuid())
        .bind(person_id.into_uuid())
        .bind(email.as_str())
        .bind(name.as_str())
        .bind(&digest.0)
        .execute(tx.conn())
        .await
        .map_err(map_identity_write_error)?;

        for role_id in &role_ids {
            sqlx::query(
                "insert into person_roles (site_id, person_id, role_id)
                 values ($1, $2, $3)",
            )
            .bind(context.site_id.into_uuid())
            .bind(person_id.into_uuid())
            .bind(role_id.into_uuid())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }

        let person = self.get_person(tx, context, person_id).await?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "people.person.created".to_owned(),
                    resource_type: "Person".to_owned(),
                    resource_id: Some(person_id.into_uuid()),
                    payload: serde_json::json!({"role_count": role_ids.len()}),
                },
            )
            .await?;
        Ok(person)
    }

    pub async fn update_person_status(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        person_id: PersonId,
        input: &UpdatePersonStatus,
        now: DateTime<Utc>,
    ) -> Result<PersonRecord> {
        require_context_grant(context, Grant::new(Capability::People, Action::Write))?;
        if matches!(
            context.caller,
            Caller::Account {
                person_id: current_id,
                ..
            } if current_id == person_id && input.status != PersonStatus::Active
        ) {
            return Err(MaviError::conflict("cannot_deactivate_current_person"));
        }

        let affected = sqlx::query(
            "update people set status = $3, updated_at = $4
               where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(person_id.into_uuid())
        .bind(input.status.as_str())
        .bind(now)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .rows_affected();
        if affected == 0 {
            return Err(MaviError::NotFound {
                resource: PERSON_NOT_FOUND,
            });
        }
        if input.status != PersonStatus::Active {
            sqlx::query(
                "update sessions set revoked_at = $3
                   where site_id = $1 and person_id = $2 and revoked_at is null",
            )
            .bind(context.site_id.into_uuid())
            .bind(person_id.into_uuid())
            .bind(now)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            sqlx::query(
                "update api_keys set revoked_at = $3
                   where site_id = $1 and person_id = $2 and revoked_at is null",
            )
            .bind(context.site_id.into_uuid())
            .bind(person_id.into_uuid())
            .bind(now)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }

        let person = self.get_person(tx, context, person_id).await?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "people.person.status_updated".to_owned(),
                    resource_type: "Person".to_owned(),
                    resource_id: Some(person_id.into_uuid()),
                    payload: serde_json::json!({"status": input.status}),
                },
            )
            .await?;
        Ok(person)
    }

    pub async fn replace_person_roles(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        person_id: PersonId,
        input: &ReplacePersonRoles,
        now: DateTime<Utc>,
    ) -> Result<PersonRecord> {
        require_context_grant(context, Grant::new(Capability::People, Action::Write))?;
        let changes_current_person = match &context.caller {
            Caller::Account {
                person_id: current_id,
                ..
            }
            | Caller::Assistant {
                person_id: Some(current_id),
                ..
            } => *current_id == person_id,
            _ => false,
        };
        if changes_current_person {
            return Err(MaviError::conflict("cannot_change_current_person_roles"));
        }

        let role_ids = unique_role_ids(&input.role_ids);
        ensure_roles_are_delegable(tx, context, &role_ids).await?;

        sqlx::query(
            "select id from people
              where site_id = $1 and id = $2
              for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(person_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: PERSON_NOT_FOUND,
        })?;

        sqlx::query("delete from person_roles where site_id = $1 and person_id = $2")
            .bind(context.site_id.into_uuid())
            .bind(person_id.into_uuid())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;

        for role_id in &role_ids {
            sqlx::query(
                "insert into person_roles (site_id, person_id, role_id)
                 values ($1, $2, $3)",
            )
            .bind(context.site_id.into_uuid())
            .bind(person_id.into_uuid())
            .bind(role_id.into_uuid())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }

        sqlx::query("update people set updated_at = $3 where site_id = $1 and id = $2")
            .bind(context.site_id.into_uuid())
            .bind(person_id.into_uuid())
            .bind(now)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;

        let person = self.get_person(tx, context, person_id).await?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "people.person.roles_replaced".to_owned(),
                    resource_type: "Person".to_owned(),
                    resource_id: Some(person_id.into_uuid()),
                    payload: serde_json::json!({"role_count": role_ids.len()}),
                },
            )
            .await?;
        Ok(person)
    }

    pub async fn list_roles(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &RoleListFilter,
    ) -> Result<Page<Role>> {
        require_context_grant(context, Grant::new(Capability::People, Action::View))?;
        let after = filter
            .page
            .after
            .as_ref()
            .map(decode_identity_cursor)
            .transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, site_id, name, created_at, system_role as protected
               from roles where site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        if let Some(after) = after {
            query
                .push(" and (created_at, id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        let rows = query
            .push(" order by created_at desc, id desc limit ")
            .push_bind(limit + 1)
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let limit_usize = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if rows.len() > limit_usize {
            let row = rows
                .get(limit_usize.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_identity_cursor(
                row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                row.try_get("id").map_err(|_| MaviError::Internal)?,
            )?)
        } else {
            None
        };
        let mut items = Vec::with_capacity(rows.len().min(limit_usize));
        for row in rows.into_iter().take(limit_usize) {
            let role_id = RoleId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
            let grants = grants_for_role(tx, context.site_id, role_id).await?;
            items.push(role_from_row(&row, grants)?);
        }
        Ok(Page::new(items, next_cursor))
    }

    pub async fn create_role(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateRole,
    ) -> Result<Role> {
        require_context_grant(context, Grant::new(Capability::People, Action::Write))?;
        let name = RoleName::parse(&input.name)?;
        let grants = delegated_grants(context, &input.grants)?;
        let role_id = RoleId::new();
        let row = sqlx::query(
            "insert into roles (site_id, id, name) values ($1, $2, $3)
             returning id, site_id, name, created_at, system_role as protected",
        )
        .bind(context.site_id.into_uuid())
        .bind(role_id.into_uuid())
        .bind(name.as_str())
        .fetch_one(tx.conn())
        .await
        .map_err(map_identity_write_error)?;
        insert_role_grants(tx, context.site_id, role_id, &grants).await?;
        let role = role_from_row(&row, Grants::new(grants.clone()))?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "people.role.created".to_owned(),
                    resource_type: "Role".to_owned(),
                    resource_id: Some(role_id.into_uuid()),
                    payload: serde_json::json!({"grant_count": grants.len()}),
                },
            )
            .await?;
        Ok(role)
    }

    pub async fn delete_role(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        role_id: RoleId,
    ) -> Result<()> {
        require_context_grant(context, Grant::new(Capability::People, Action::Delete))?;

        let row = sqlx::query(
            "select name, system_role
               from roles
              where site_id = $1 and id = $2
              for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(role_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: ROLE_NOT_FOUND,
        })?;

        let name: String = row.try_get("name").map_err(|_| MaviError::Internal)?;
        let protected: bool = row
            .try_get("system_role")
            .map_err(|_| MaviError::Internal)?;
        if protected || name == "owner" {
            return Err(MaviError::conflict(OWNER_ROLE_PROTECTED));
        }

        let assigned: bool = sqlx::query_scalar(
            "select exists(
                 select 1 from person_roles
                  where site_id = $1 and role_id = $2
             )",
        )
        .bind(context.site_id.into_uuid())
        .bind(role_id.into_uuid())
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if assigned {
            return Err(MaviError::conflict(ROLE_ASSIGNED));
        }

        sqlx::query("delete from role_grants where site_id = $1 and role_id = $2")
            .bind(context.site_id.into_uuid())
            .bind(role_id.into_uuid())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        sqlx::query("delete from roles where site_id = $1 and id = $2")
            .bind(context.site_id.into_uuid())
            .bind(role_id.into_uuid())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "people.role.deleted".to_owned(),
                    resource_type: "Role".to_owned(),
                    resource_id: Some(role_id.into_uuid()),
                    payload: serde_json::json!({"name": name}),
                },
            )
            .await?;
        Ok(())
    }

    pub async fn replace_role_grants(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        role_id: RoleId,
        input: &ReplaceRoleGrants,
    ) -> Result<Role> {
        require_context_grant(context, Grant::new(Capability::People, Action::Write))?;
        let grants = delegated_grants(context, &input.grants)?;
        let row = sqlx::query(
            "select name, system_role
               from roles
              where site_id = $1 and id = $2
              for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(role_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: ROLE_NOT_FOUND,
        })?;
        let role_name: String = row.try_get("name").map_err(|_| MaviError::Internal)?;
        let protected: bool = row
            .try_get("system_role")
            .map_err(|_| MaviError::Internal)?;
        if protected || role_name == "owner" {
            return Err(MaviError::conflict(OWNER_ROLE_PROTECTED));
        }
        sqlx::query("delete from role_grants where site_id = $1 and role_id = $2")
            .bind(context.site_id.into_uuid())
            .bind(role_id.into_uuid())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        insert_role_grants(tx, context.site_id, role_id, &grants).await?;
        let role = get_role(tx, context.site_id, role_id).await?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "people.role.grants_replaced".to_owned(),
                    resource_type: "Role".to_owned(),
                    resource_id: Some(role_id.into_uuid()),
                    payload: serde_json::json!({"grant_count": grants.len()}),
                },
            )
            .await?;
        Ok(role)
    }

    pub async fn list_api_keys(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &ApiKeyListFilter,
    ) -> Result<Page<ApiKeyRecord>> {
        if !matches!(context.caller, Caller::Account { .. }) {
            return Err(MaviError::Forbidden);
        }
        require_context_grant(context, Grant::new(Capability::People, Action::View))?;
        let after = filter
            .page
            .after
            .as_ref()
            .map(decode_identity_cursor)
            .transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, site_id, person_id, name, prefix, expires_at, revoked_at, created_at
               from api_keys where site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        match filter.revoked {
            Some(true) => query.push(" and revoked_at is not null"),
            Some(false) => query.push(" and revoked_at is null"),
            None => &mut query,
        };
        if let Some(after) = after {
            query
                .push(" and (created_at, id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        let rows = query
            .push(" order by created_at desc, id desc limit ")
            .push_bind(limit + 1)
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let limit_usize = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if rows.len() > limit_usize {
            let row = rows
                .get(limit_usize.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_identity_cursor(
                row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                row.try_get("id").map_err(|_| MaviError::Internal)?,
            )?)
        } else {
            None
        };
        let mut items = Vec::with_capacity(rows.len().min(limit_usize));
        for row in rows.into_iter().take(limit_usize) {
            let key_id = ApiKeyId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
            let grants = grants_for_api_key(tx, context.site_id, key_id).await?;
            items.push(api_key_from_row(&row, grants)?);
        }
        Ok(Page::new(items, next_cursor))
    }

    pub async fn create_api_key(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateApiKey,
        now: DateTime<Utc>,
    ) -> Result<ApiKeyCreated> {
        require_context_grant(context, Grant::new(Capability::People, Action::Write))?;
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
        let row = sqlx::query(
            "insert into api_keys (site_id, id, person_id, name, prefix, secret_hash, expires_at)
             values ($1, $2, $3, $4, $5, $6, $7)
             returning created_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(api_key_id.into_uuid())
        .bind(person_id.into_uuid())
        .bind(name)
        .bind(prefix)
        .bind(hash_token(&token))
        .bind(input.expires_at)
        .fetch_one(tx.conn())
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
                    action: audit_action::API_KEY_CREATED.to_owned(),
                    resource_type: "ApiKey".to_owned(),
                    resource_id: Some(api_key_id.into_uuid()),
                    payload: serde_json::json!({"grant_count": requested.len()}),
                },
            )
            .await?;

        Ok(ApiKeyCreated {
            id: api_key_id,
            site_id: context.site_id,
            person_id: *person_id,
            name: name.to_owned(),
            prefix: prefix.to_owned(),
            token,
            grants: Grants::new(requested),
            expires_at: input.expires_at,
            created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        })
    }

    pub async fn revoke_api_key(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        key_id: ApiKeyId,
        now: DateTime<Utc>,
    ) -> Result<()> {
        require_context_grant(context, Grant::new(Capability::People, Action::Delete))?;
        match &context.caller {
            Caller::Assistant {
                key_id: caller_key_id,
                ..
            } if *caller_key_id != key_id => {
                return Err(MaviError::Forbidden);
            }
            _ => {}
        }

        let row = sqlx::query(
            "select revoked_at
               from api_keys
              where site_id = $1 and id = $2
              for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(key_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: API_KEY_NOT_FOUND,
        })?;

        let revoked_at: Option<DateTime<Utc>> =
            row.try_get("revoked_at").map_err(|_| MaviError::Internal)?;
        if revoked_at.is_some() {
            return Err(MaviError::NotFound {
                resource: API_KEY_NOT_FOUND,
            });
        }

        sqlx::query(
            "update api_keys set revoked_at = $3
               where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(key_id.into_uuid())
        .bind(now)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: audit_action::API_KEY_REVOKED.to_owned(),
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

#[derive(Clone, Debug, Deserialize, Serialize)]
struct IdentityCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

fn encode_identity_cursor(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes =
        serde_json::to_vec(&IdentityCursor { created_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

async fn lock_site(tx: &mut SiteTx, site_id: SiteId) -> Result<()> {
    let site_id: Option<Uuid> =
        sqlx::query_scalar("select site_id from site_catalog where site_id = $1 for update")
            .bind(site_id.into_uuid())
            .fetch_optional(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
    if site_id.is_none() {
        return Err(MaviError::NotFound {
            resource: SITE_NOT_FOUND,
        });
    }
    Ok(())
}

fn decode_identity_cursor(cursor: &Cursor) -> Result<IdentityCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

fn person_from_row(row: &sqlx::postgres::PgRow) -> Result<PersonRecord> {
    let role_ids: Vec<Uuid> = row.try_get("role_ids").map_err(|_| MaviError::Internal)?;
    let status: String = row.try_get("status").map_err(|_| MaviError::Internal)?;
    let email: String = row.try_get("email").map_err(|_| MaviError::Internal)?;
    let name: String = row.try_get("name").map_err(|_| MaviError::Internal)?;
    let email_verified_at: Option<DateTime<Utc>> = row
        .try_get("email_verified_at")
        .map_err(|_| MaviError::Internal)?;
    Ok(PersonRecord {
        id: PersonId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        site_id: SiteId::from_uuid(row.try_get("site_id").map_err(|_| MaviError::Internal)?),
        email: Email::parse(&email).map_err(|_| MaviError::Internal)?,
        name: PersonName::parse(&name).map_err(|_| MaviError::Internal)?,
        status: PersonStatus::parse(&status)?,
        email_verified: email_verified_at.is_some(),
        role_ids: role_ids.into_iter().map(RoleId::from_uuid).collect(),
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn role_from_row(row: &sqlx::postgres::PgRow, grants: Grants) -> Result<Role> {
    let name: String = row.try_get("name").map_err(|_| MaviError::Internal)?;
    Ok(Role {
        id: RoleId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        site_id: SiteId::from_uuid(row.try_get("site_id").map_err(|_| MaviError::Internal)?),
        name: RoleName::parse(&name).map_err(|_| MaviError::Internal)?,
        grants,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        protected: row.try_get("protected").map_err(|_| MaviError::Internal)?,
    })
}

fn api_key_from_row(row: &sqlx::postgres::PgRow, grants: Grants) -> Result<ApiKeyRecord> {
    Ok(ApiKeyRecord {
        id: ApiKeyId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        site_id: SiteId::from_uuid(row.try_get("site_id").map_err(|_| MaviError::Internal)?),
        person_id: PersonId::from_uuid(row.try_get("person_id").map_err(|_| MaviError::Internal)?),
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        prefix: row.try_get("prefix").map_err(|_| MaviError::Internal)?,
        grants,
        expires_at: row.try_get("expires_at").map_err(|_| MaviError::Internal)?,
        revoked_at: row.try_get("revoked_at").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
    })
}

async fn get_role(tx: &mut SiteTx, site_id: SiteId, role_id: RoleId) -> Result<Role> {
    let row = sqlx::query(
        "select id, site_id, name, created_at, system_role as protected
           from roles
          where site_id = $1 and id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(role_id.into_uuid())
    .fetch_optional(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?
    .ok_or(MaviError::NotFound {
        resource: ROLE_NOT_FOUND,
    })?;
    let grants = grants_for_role(tx, site_id, role_id).await?;
    role_from_row(&row, grants)
}

async fn grants_for_role(tx: &mut SiteTx, site_id: SiteId, role_id: RoleId) -> Result<Grants> {
    let rows = sqlx::query(
        "select capability, action from role_grants
          where site_id = $1 and role_id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(role_id.into_uuid())
    .fetch_all(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    parse_grant_rows(rows)
}

fn delegated_grants(context: &SiteContext, requested: &[Grant]) -> Result<Vec<Grant>> {
    let held = context.caller.grants().ok_or(MaviError::Forbidden)?;
    let mut grants = Vec::new();
    for grant in requested {
        if !held.allows(*grant) {
            return Err(MaviError::Forbidden);
        }
        if !grants.contains(grant) {
            grants.push(*grant);
        }
    }
    Ok(grants)
}

async fn ensure_roles_exist(tx: &mut SiteTx, site_id: SiteId, role_ids: &[RoleId]) -> Result<()> {
    if role_ids.is_empty() {
        return Ok(());
    }
    let role_uuids: Vec<Uuid> = role_ids.iter().map(|role_id| role_id.into_uuid()).collect();
    let rows = sqlx::query(
        "select id
           from roles
          where site_id = $1 and id = any($2::uuid[])
          for share",
    )
    .bind(site_id.into_uuid())
    .bind(role_uuids)
    .fetch_all(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    if rows.len() != role_ids.len() {
        return Err(MaviError::NotFound {
            resource: ROLE_NOT_FOUND,
        });
    }
    Ok(())
}

async fn ensure_roles_are_delegable(
    tx: &mut SiteTx,
    context: &SiteContext,
    role_ids: &[RoleId],
) -> Result<()> {
    let held = context.caller.grants().ok_or(MaviError::Forbidden)?;
    ensure_roles_exist(tx, context.site_id, role_ids).await?;
    if role_ids.is_empty() {
        return Ok(());
    }

    let role_uuids: Vec<Uuid> = role_ids.iter().map(|role_id| role_id.into_uuid()).collect();
    let rows = sqlx::query(
        "select capability, action from role_grants
          where site_id = $1 and role_id = any($2::uuid[])",
    )
    .bind(context.site_id.into_uuid())
    .bind(role_uuids)
    .fetch_all(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    let grants = parse_grant_rows(rows)?;
    if grants.as_slice().iter().any(|grant| !held.allows(*grant)) {
        return Err(MaviError::Forbidden);
    }
    Ok(())
}

fn require_context_grant(context: &SiteContext, grant: Grant) -> Result<()> {
    context
        .caller
        .grants()
        .filter(|grants| grants.allows(grant))
        .map(|_| ())
        .ok_or(MaviError::Forbidden)
}

async fn insert_role_grants(
    tx: &mut SiteTx,
    site_id: SiteId,
    role_id: RoleId,
    grants: &[Grant],
) -> Result<()> {
    for grant in grants {
        sqlx::query(
            "insert into role_grants (site_id, role_id, capability, action)
             values ($1, $2, $3, $4)",
        )
        .bind(site_id.into_uuid())
        .bind(role_id.into_uuid())
        .bind(grant.capability.as_str())
        .bind(grant.action.as_str())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    }
    Ok(())
}

fn unique_role_ids(role_ids: &[RoleId]) -> Vec<RoleId> {
    let mut unique = Vec::new();
    for role_id in role_ids {
        if !unique.contains(role_id) {
            unique.push(*role_id);
        }
    }
    unique
}

fn map_identity_write_error(error: sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error {
        match database.constraint() {
            Some(
                "people_site_email_key" | "people_site_id_email_key" | "people_site_email_lower",
            ) => {
                return MaviError::conflict("email_taken");
            }
            Some("roles_site_name_key" | "roles_site_id_name_key") => {
                return MaviError::conflict("role_name_taken");
            }
            _ => {}
        }
    }
    MaviError::Internal
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

    parse_grant_rows(rows)
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

    parse_grant_rows(rows)
}

fn parse_grant_rows(rows: Vec<sqlx::postgres::PgRow>) -> Result<Grants> {
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

fn new_password_reset_token() -> String {
    format!("mavi_reset_{}", new_token())
}

fn new_email_verification_token() -> String {
    format!("mavi_verify_{}", new_token())
}

fn api_key_prefix(token: &str) -> Option<&str> {
    token.strip_prefix("mavi_key_")?;
    token.get(..16)
}

fn hash_token(token: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};

    Sha256::digest(token.as_bytes()).to_vec()
}

async fn record_auth_audit(
    tx: &mut SiteTx,
    context: &SiteContext,
    action: &str,
    resource_type: &str,
    resource_id: Option<Uuid>,
    payload: serde_json::Value,
) -> Result<()> {
    AuditService
        .record(
            tx,
            context,
            &AuditEntry {
                action: action.to_owned(),
                resource_type: resource_type.to_owned(),
                resource_id,
                payload,
            },
        )
        .await
}

async fn issue_password_reset_token(
    tx: &mut SiteTx,
    site_id: SiteId,
    person_id: PersonId,
    now: DateTime<Utc>,
) -> Result<(PasswordResetTokenId, String, DateTime<Utc>)> {
    let token_id = PasswordResetTokenId::new();
    let token = new_password_reset_token();
    let expires_at = now + PASSWORD_RESET_TTL;
    sqlx::query(
        "update password_reset_tokens
            set revoked_at = $3
          where site_id = $1 and person_id = $2
            and used_at is null and revoked_at is null",
    )
    .bind(site_id.into_uuid())
    .bind(person_id.into_uuid())
    .bind(now)
    .execute(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    sqlx::query(
        "insert into password_reset_tokens
            (site_id, id, person_id, token_hash, expires_at)
         values ($1, $2, $3, $4, $5)",
    )
    .bind(site_id.into_uuid())
    .bind(token_id.into_uuid())
    .bind(person_id.into_uuid())
    .bind(hash_token(&token))
    .bind(expires_at)
    .execute(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    Ok((token_id, token, expires_at))
}

async fn issue_email_verification_token(
    tx: &mut SiteTx,
    site_id: SiteId,
    person_id: PersonId,
    now: DateTime<Utc>,
) -> Result<(EmailVerificationTokenId, String, DateTime<Utc>)> {
    let token_id = EmailVerificationTokenId::new();
    let token = new_email_verification_token();
    let expires_at = now + EMAIL_VERIFICATION_TTL;
    sqlx::query(
        "update email_verification_tokens
            set revoked_at = $3
          where site_id = $1 and person_id = $2
            and used_at is null and revoked_at is null",
    )
    .bind(site_id.into_uuid())
    .bind(person_id.into_uuid())
    .bind(now)
    .execute(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    sqlx::query(
        "insert into email_verification_tokens
            (site_id, id, person_id, token_hash, expires_at)
         values ($1, $2, $3, $4, $5)",
    )
    .bind(site_id.into_uuid())
    .bind(token_id.into_uuid())
    .bind(person_id.into_uuid())
    .bind(hash_token(&token))
    .bind(expires_at)
    .execute(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    Ok((token_id, token, expires_at))
}

async fn allow_auth_request(
    tx: &mut SiteTx,
    site_id: SiteId,
    action: &str,
    subject_hash: Vec<u8>,
    now: DateTime<Utc>,
) -> Result<bool> {
    let row = sqlx::query(
        "select window_started_at, request_count
           from auth_request_throttles
          where site_id = $1 and action = $2 and subject_hash = $3
          for update",
    )
    .bind(site_id.into_uuid())
    .bind(action)
    .bind(&subject_hash)
    .fetch_optional(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;

    let Some(row) = row else {
        sqlx::query(
            "insert into auth_request_throttles
                (site_id, action, subject_hash, window_started_at, request_count)
             values ($1, $2, $3, $4, 1)",
        )
        .bind(site_id.into_uuid())
        .bind(action)
        .bind(subject_hash)
        .bind(now)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        return Ok(true);
    };

    let window_started_at: DateTime<Utc> = row
        .try_get("window_started_at")
        .map_err(|_| MaviError::Internal)?;
    let request_count: i32 = row
        .try_get("request_count")
        .map_err(|_| MaviError::Internal)?;
    if now.signed_duration_since(window_started_at) >= AUTH_REQUEST_WINDOW {
        sqlx::query(
            "update auth_request_throttles
                set window_started_at = $4, request_count = 1, updated_at = $4
              where site_id = $1 and action = $2 and subject_hash = $3",
        )
        .bind(site_id.into_uuid())
        .bind(action)
        .bind(subject_hash)
        .bind(now)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        return Ok(true);
    }
    if request_count >= MAX_AUTH_REQUESTS_PER_WINDOW {
        return Ok(false);
    }

    sqlx::query(
        "update auth_request_throttles
            set request_count = request_count + 1, updated_at = $4
          where site_id = $1 and action = $2 and subject_hash = $3",
    )
    .bind(site_id.into_uuid())
    .bind(action)
    .bind(subject_hash)
    .bind(now)
    .execute(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_is_normalized_before_storage() {
        let email = Email::parse("  Owner@Example.COM ").expect("valid email");
        assert_eq!(email.as_str(), "owner@example.com");
        assert!(Email::parse("not-an-email").is_err());
        assert!(Email::parse(&format!("owner@{}@other.com", "example.com")).is_err());
        assert!(Email::parse(&format!("owner@-{}", "example.com")).is_err());
    }

    #[test]
    fn password_policy_rejects_short_values() {
        assert!(Password::parse("too-short".to_owned()).is_err());
        assert!(Password::parse("long-enough-password".to_owned()).is_ok());
    }

    #[test]
    fn setup_debug_output_redacts_passwords() {
        let input = SetupInput {
            site_name: "Mavi".to_owned(),
            email: "owner@example.com".to_owned(),
            name: "Owner".to_owned(),
            password: "super-secret-password".to_owned(),
        };
        let debug = format!("{input:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("super-secret-password"));
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
    fn password_reset_tokens_are_prefixed_and_have_no_plaintext_hash_round_trip() {
        let token = new_password_reset_token();
        assert!(token.starts_with("mavi_reset_"));
        assert_eq!(hash_token(&token), hash_token(&token));
        assert_ne!(hash_token(&token), token.as_bytes());
    }

    #[test]
    fn password_reset_redeem_debug_output_redacts_secrets() {
        let input = PasswordResetRedeemInput {
            token: "mavi_reset_secret".to_owned(),
            password: "super-secret-password".to_owned(),
        };
        let debug = format!("{input:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("mavi_reset_secret"));
        assert!(!debug.contains("super-secret-password"));
    }

    #[test]
    fn email_verification_tokens_are_prefixed_and_hashed() {
        let token = new_email_verification_token();
        assert!(token.starts_with("mavi_verify_"));
        assert_eq!(hash_token(&token), hash_token(&token));
        assert_ne!(hash_token(&token), token.as_bytes());
    }

    #[test]
    fn email_verification_redeem_debug_output_redacts_secrets() {
        let input = EmailVerificationRedeemInput {
            token: "mavi_verify_secret".to_owned(),
        };
        let debug = format!("{input:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("mavi_verify_secret"));
    }

    #[test]
    fn role_names_are_lowercase_delegation_safe_identifiers() {
        assert!(RoleName::parse("editor").is_ok());
        assert!(RoleName::parse("Editor").is_err());
        assert!(RoleName::parse("content-editor").is_ok());
        assert!(RoleName::parse("owner role").is_err());
    }

    #[test]
    fn delegated_role_grants_cannot_escalate_the_caller() {
        let grant = Grant::new(Capability::Content, Action::View);
        let context = SiteContext::with_caller(
            SiteId::new(),
            Caller::Account {
                person_id: PersonId::new(),
                session_id: None,
                grants: Grants::new([grant]),
            },
            mavi_core::RequestId::new(),
        );

        assert!(delegated_grants(&context, &[grant]).is_ok());
        assert!(
            delegated_grants(&context, &[Grant::new(Capability::Content, Action::Write)]).is_err()
        );
    }

    #[test]
    fn identity_cursor_round_trips_and_rejects_corruption() {
        let id = Uuid::now_v7();
        let created_at = Utc::now();
        let cursor = encode_identity_cursor(created_at, id).expect("cursor");
        let decoded = decode_identity_cursor(&cursor).expect("decoded cursor");

        assert_eq!(decoded.id, id);
        assert_eq!(decoded.created_at, created_at);
        assert!(decode_identity_cursor(&Cursor::parse("bad").expect("cursor")).is_err());
    }

    #[test]
    fn identity_api_is_valid() {
        api().validate().expect("identity API contract is valid");
    }
}
