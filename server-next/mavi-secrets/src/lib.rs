//! Site-scoped provider credentials.
//!
//! Credential values are accepted only at this application boundary, sealed
//! through the [`mavi_core::ports::Seals`] adapter and never returned by the
//! HTTP API. `PostgreSQL` stores ciphertext and metadata; audit payloads contain
//! provider/name/version information only. Provider workers can explicitly
//! request a short-lived [`CredentialMaterial`] through the trusted port.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, CredentialId, Cursor, ErrorCode, Grant, MaviError, Page, PageRequest,
    Result, SiteContext, SiteId, ports::Seals,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

pub const CREDENTIAL_NOT_FOUND: &str = "credential_not_found";
pub const CREDENTIAL_PROVIDER_INVALID: &str = "credential_provider_invalid";
pub const CREDENTIAL_NAME_INVALID: &str = "credential_name_invalid";
pub const CREDENTIAL_VALUES_INVALID: &str = "credential_values_invalid";
pub const CREDENTIAL_ALREADY_EXISTS: &str = "credential_already_exists";
pub const CREDENTIAL_VERSION_CONFLICT: &str = "credential_version_conflict";
pub const CREDENTIAL_REVOKED: &str = "credential_revoked";
pub const CREDENTIAL_PAYLOAD_TOO_LARGE: &str = "credential_payload_too_large";

const SEALED_PAYLOAD_VERSION: u16 = 1;
const MAX_IDENTIFIER_CHARS: usize = 120;
const MAX_PROVIDER_CHARS: usize = 64;
const MAX_VALUE_KEY_CHARS: usize = 64;
const MAX_VALUES: usize = 64;
const MAX_VALUE_BYTES: usize = 16 * 1024;
const MAX_SEALED_PAYLOAD_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CredentialListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

/// Plaintext is accepted only for the create/rotate command and is never
/// serialized back as a response. The map allows provider adapters to define
/// their own fields without making the core domain depend on SMTP/Stripe/etc.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCredential {
    pub provider: String,
    pub name: String,
    pub values: BTreeMap<String, String>,
}

impl fmt::Debug for CreateCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CreateCredential")
            .field("provider", &self.provider)
            .field("name", &self.name)
            .field("values", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RotateCredential {
    pub expected_version: i64,
    pub values: BTreeMap<String, String>,
}

impl fmt::Debug for RotateCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RotateCredential")
            .field("expected_version", &self.expected_version)
            .field("values", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialState {
    Active,
    Revoked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Credential {
    pub id: CredentialId,
    pub site_id: SiteId,
    pub provider: String,
    pub name: String,
    pub state: CredentialState,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Decrypted provider material for trusted adapters only.
///
/// This type intentionally does not implement `Serialize`, and its debug
/// representation redacts every value. Callers should drop it immediately
/// after constructing a provider request.
#[derive(Clone, Eq, PartialEq)]
pub struct CredentialMaterial {
    pub id: CredentialId,
    pub provider: String,
    pub name: String,
    pub version: i64,
    values: BTreeMap<String, String>,
}

impl fmt::Debug for CredentialMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialMaterial")
            .field("id", &self.id)
            .field("provider", &self.provider)
            .field("name", &self.name)
            .field("version", &self.version)
            .field("values", &"<redacted>")
            .finish()
    }
}

impl CredentialMaterial {
    #[must_use]
    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CredentialService;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct SealedCredentialPayload {
    version: u16,
    values: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CredentialCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

#[must_use]
pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new([
        Endpoint::new(
            Method::Get,
            "/api/v1/credentials",
            "credentials.list",
            "List provider credential metadata",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Credentials,
            action: Action::View,
        })
        .takes_query("CredentialListFilter")
        .returns(200, "CredentialPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/credentials",
            "credentials.create",
            "Create a sealed provider credential",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Credentials,
            action: Action::Write,
        })
        .takes("CreateCredential")
        .returns(201, "Credential")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Put,
            "/api/v1/credentials/{id}",
            "credentials.rotate",
            "Rotate a provider credential without exposing its value",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Credentials,
            action: Action::Write,
        })
        .takes("RotateCredential")
        .returns(200, "Credential")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/credentials/{id}",
            "credentials.revoke",
            "Revoke a provider credential",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Credentials,
            action: Action::Delete,
        })
        .returns(204, "Empty")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
    ])
    .with_shapes(shapes())
}

#[allow(clippy::too_many_lines)]
fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "CredentialListFilter",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100}
                }
            }),
        ),
        Shape::new(
            "CreateCredential",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["provider", "name", "values"],
                "properties": {
                    "provider": {"type": "string", "pattern": "^[a-z][a-z0-9_-]{0,63}$"},
                    "name": {"type": "string", "pattern": "^[a-z][a-z0-9_-]{0,119}$"},
                    "values": {"type": "object", "additionalProperties": {"type": "string"}, "minProperties": 1}
                }
            }),
        ),
        Shape::new(
            "RotateCredential",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["expected_version", "values"],
                "properties": {
                    "expected_version": {"type": "integer", "minimum": 1},
                    "values": {"type": "object", "additionalProperties": {"type": "string"}, "minProperties": 1}
                }
            }),
        ),
        Shape::new(
            "Credential",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "site_id", "provider", "name", "state", "version", "created_at", "updated_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "site_id": {"type": "string", "format": "uuid"},
                    "provider": {"type": "string"},
                    "name": {"type": "string"},
                    "state": {"type": "string", "enum": ["active", "revoked"]},
                    "version": {"type": "integer", "minimum": 1},
                    "created_at": {"type": "string", "format": "date-time"},
                    "updated_at": {"type": "string", "format": "date-time"}
                }
            }),
        ),
        Shape::new(
            "CredentialPage",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/Credential"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512}
                }
            }),
        ),
    ]
}

impl CredentialService {
    pub async fn list(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &CredentialListFilter,
    ) -> Result<Page<Credential>> {
        require_grant(context, Action::View)?;
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, site_id, provider, name, revoked_at, version, created_at, updated_at
               from site_credentials where site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        if let Some(after) = after {
            query
                .push(" and (created_at, id) > (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        query
            .push(" order by created_at asc, id asc limit ")
            .push_bind(limit + 1);
        let rows = query
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let items = rows
            .iter()
            .take(filter.page.effective_limit() as usize)
            .map(credential_from_row)
            .collect::<Result<Vec<_>>>()?;
        let next_cursor = if rows.len() > items.len() {
            items
                .last()
                .map(|item| encode_cursor(item.created_at, item.id.into_uuid()))
        } else {
            None
        };
        Ok(Page::new(items, next_cursor.transpose()?))
    }

    pub async fn create(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        sealer: &dyn Seals,
        input: &CreateCredential,
    ) -> Result<Credential> {
        require_grant(context, Action::Write)?;
        let provider = validate_provider(&input.provider)?;
        let name = validate_name(&input.name)?;
        let payload = seal_payload(context, sealer, &input.values).await?;
        let id = CredentialId::new();
        let result = sqlx::query(
            "insert into site_credentials
                (site_id, id, provider, name, sealed_payload)
             values ($1, $2, $3, $4, $5)
             returning id, site_id, provider, name, revoked_at, version, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(&provider)
        .bind(&name)
        .bind(payload)
        .fetch_one(tx.conn())
        .await;
        let row = match result {
            Ok(row) => row,
            Err(error)
                if error
                    .as_database_error()
                    .and_then(|database| database.constraint())
                    == Some("site_credentials_active_name") =>
            {
                return Err(MaviError::conflict(CREDENTIAL_ALREADY_EXISTS));
            }
            Err(_) => return Err(MaviError::Internal),
        };
        let credential = credential_from_row(&row)?;
        audit(
            tx,
            context,
            "credentials.created",
            credential.id,
            json!({"provider": credential.provider, "name": credential.name, "version": credential.version, "value_count": input.values.len()}),
        )
        .await?;
        Ok(credential)
    }

    pub async fn rotate(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        sealer: &dyn Seals,
        id: CredentialId,
        input: &RotateCredential,
    ) -> Result<Credential> {
        require_grant(context, Action::Write)?;
        if input.expected_version < 1 {
            return Err(MaviError::validation_field(
                "credential_version_invalid",
                "expected_version",
            ));
        }
        let payload = seal_payload(context, sealer, &input.values).await?;
        let current = sqlx::query(
            "select provider, name, revoked_at, version
               from site_credentials where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: CREDENTIAL_NOT_FOUND,
        })?;
        let revoked_at: Option<DateTime<Utc>> = current
            .try_get("revoked_at")
            .map_err(|_| MaviError::Internal)?;
        if revoked_at.is_some() {
            return Err(MaviError::conflict(CREDENTIAL_REVOKED));
        }
        let version: i64 = current
            .try_get("version")
            .map_err(|_| MaviError::Internal)?;
        if version != input.expected_version {
            return Err(MaviError::conflict(CREDENTIAL_VERSION_CONFLICT));
        }

        let row = sqlx::query(
            "update site_credentials
                set sealed_payload = $3, version = version + 1, updated_at = now()
              where site_id = $1 and id = $2 and version = $4 and revoked_at is null
             returning id, site_id, provider, name, revoked_at, version, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(payload)
        .bind(input.expected_version)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or_else(|| MaviError::conflict(CREDENTIAL_VERSION_CONFLICT))?;
        let credential = credential_from_row(&row)?;
        audit(
            tx,
            context,
            "credentials.rotated",
            id,
            json!({"provider": credential.provider, "name": credential.name, "version": credential.version}),
        )
        .await?;
        Ok(credential)
    }

    pub async fn revoke(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: CredentialId,
    ) -> Result<()> {
        require_grant(context, Action::Delete)?;
        let row = sqlx::query(
            "select provider, name, revoked_at, version
               from site_credentials where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: CREDENTIAL_NOT_FOUND,
        })?;
        if row
            .try_get::<Option<DateTime<Utc>>, _>("revoked_at")
            .map_err(|_| MaviError::Internal)?
            .is_some()
        {
            return Ok(());
        }
        let provider: String = row.try_get("provider").map_err(|_| MaviError::Internal)?;
        let name: String = row.try_get("name").map_err(|_| MaviError::Internal)?;
        sqlx::query(
            "update site_credentials
                set revoked_at = now(), version = version + 1, updated_at = now()
              where site_id = $1 and id = $2 and revoked_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        audit(
            tx,
            context,
            "credentials.revoked",
            id,
            json!({"provider": provider, "name": name}),
        )
        .await
    }

    /// Opens credential material for a trusted provider adapter. The HTTP
    /// layer never exposes this method or its result.
    pub async fn unseal(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        sealer: &dyn Seals,
        id: CredentialId,
    ) -> Result<CredentialMaterial> {
        require_grant(context, Action::View)?;
        let row = sqlx::query(
            "select provider, name, sealed_payload, revoked_at, version
               from site_credentials where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: CREDENTIAL_NOT_FOUND,
        })?;
        if row
            .try_get::<Option<DateTime<Utc>>, _>("revoked_at")
            .map_err(|_| MaviError::Internal)?
            .is_some()
        {
            return Err(MaviError::conflict(CREDENTIAL_REVOKED));
        }
        let provider: String = row.try_get("provider").map_err(|_| MaviError::Internal)?;
        let name: String = row.try_get("name").map_err(|_| MaviError::Internal)?;
        let version: i64 = row.try_get("version").map_err(|_| MaviError::Internal)?;
        let sealed_bytes: Vec<u8> = row
            .try_get("sealed_payload")
            .map_err(|_| MaviError::Internal)?;
        let payload = sealer.unseal(context, &sealed_bytes).await?;
        let payload: SealedCredentialPayload =
            serde_json::from_slice(&payload).map_err(|_| MaviError::Internal)?;
        if payload.version != SEALED_PAYLOAD_VERSION {
            return Err(MaviError::Internal);
        }
        validate_values(&payload.values)?;
        audit(
            tx,
            context,
            "credentials.unsealed",
            id,
            json!({"provider": provider, "name": name, "version": version}),
        )
        .await?;
        Ok(CredentialMaterial {
            id,
            provider,
            name,
            version,
            values: payload.values,
        })
    }
}

fn require_grant(context: &SiteContext, action: Action) -> Result<()> {
    let grant = Grant::new(Capability::Credentials, action);
    if context
        .caller
        .grants()
        .is_some_and(|grants| grants.allows(grant))
    {
        return Ok(());
    }
    if context.caller.is_public() {
        Err(MaviError::Unauthenticated)
    } else {
        Err(MaviError::Forbidden)
    }
}

async fn seal_payload(
    context: &SiteContext,
    sealer: &dyn Seals,
    values: &BTreeMap<String, String>,
) -> Result<Vec<u8>> {
    validate_values(values)?;
    let payload = serde_json::to_vec(&SealedCredentialPayload {
        version: SEALED_PAYLOAD_VERSION,
        values: values.clone(),
    })
    .map_err(|_| MaviError::Internal)?;
    if payload.len() > MAX_SEALED_PAYLOAD_BYTES {
        return Err(MaviError::validation(CREDENTIAL_PAYLOAD_TOO_LARGE));
    }
    sealer.seal(context, &payload).await
}

fn validate_provider(value: &str) -> Result<String> {
    validate_identifier(value, MAX_PROVIDER_CHARS, CREDENTIAL_PROVIDER_INVALID)
}

fn validate_name(value: &str) -> Result<String> {
    validate_identifier(value, MAX_IDENTIFIER_CHARS, CREDENTIAL_NAME_INVALID)
}

fn validate_identifier(value: &str, max_chars: usize, code: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    let valid = !value.is_empty()
        && value.chars().count() <= max_chars
        && value.starts_with(|character: char| character.is_ascii_lowercase())
        && value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '_'
                || character == '-'
        });
    if valid {
        Ok(value)
    } else {
        Err(MaviError::validation(code))
    }
}

fn validate_values(values: &BTreeMap<String, String>) -> Result<()> {
    if values.is_empty() || values.len() > MAX_VALUES {
        return Err(MaviError::validation(CREDENTIAL_VALUES_INVALID));
    }
    let mut keys = BTreeSet::new();
    for (key, value) in values {
        if key.is_empty()
            || key.chars().count() > MAX_VALUE_KEY_CHARS
            || !key.starts_with(|character: char| character.is_ascii_lowercase())
            || !key.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || character == '_'
                    || character == '-'
                    || character == '.'
            })
            || !keys.insert(key)
            || value.is_empty()
            || value.len() > MAX_VALUE_BYTES
        {
            return Err(MaviError::validation(CREDENTIAL_VALUES_INVALID));
        }
    }
    Ok(())
}

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes = serde_json::to_vec(&CredentialCursor { created_at, id })
        .map_err(|_| MaviError::Internal)?;
    Cursor::parse(base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        bytes,
    ))
}

fn decode_cursor(cursor: &Cursor) -> Result<CredentialCursor> {
    let bytes = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        cursor.as_str(),
    )
    .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

fn credential_from_row(row: &sqlx::postgres::PgRow) -> Result<Credential> {
    let version: i64 = row.try_get("version").map_err(|_| MaviError::Internal)?;
    if version < 1 {
        return Err(MaviError::Internal);
    }
    let revoked_at: Option<DateTime<Utc>> =
        row.try_get("revoked_at").map_err(|_| MaviError::Internal)?;
    Ok(Credential {
        id: CredentialId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        site_id: SiteId::from_uuid(row.try_get("site_id").map_err(|_| MaviError::Internal)?),
        provider: row.try_get("provider").map_err(|_| MaviError::Internal)?,
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        state: if revoked_at.is_some() {
            CredentialState::Revoked
        } else {
            CredentialState::Active
        },
        version,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

async fn audit(
    tx: &mut SiteTx,
    context: &SiteContext,
    action: &str,
    id: CredentialId,
    payload: serde_json::Value,
) -> Result<()> {
    AuditService
        .record(
            tx,
            context,
            &AuditEntry {
                action: action.to_owned(),
                resource_type: "Credential".to_owned(),
                resource_id: Some(id.into_uuid()),
                payload,
            },
        )
        .await
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use mavi_contract::Api;

    use super::{CreateCredential, CredentialMaterial, RotateCredential, api};

    #[test]
    fn credential_contract_is_metadata_only_and_cursor_based() {
        let contract: Api = api();
        contract.validate().expect("credential contract");
        let serialized = serde_json::to_string(&contract).expect("contract json");
        assert!(serialized.contains("credentials.list"));
        assert!(serialized.contains("CredentialPage"));
        assert!(!serialized.contains("sealed_payload"));
        assert!(!serialized.contains("offset"));
    }

    #[test]
    fn secret_commands_and_material_redact_values_in_debug() {
        let mut values = BTreeMap::new();
        values.insert("api_key".to_owned(), "do-not-log-me".to_owned());
        let create = CreateCredential {
            provider: "mail".to_owned(),
            name: "primary".to_owned(),
            values: values.clone(),
        };
        let rotate = RotateCredential {
            expected_version: 1,
            values,
        };
        let material = CredentialMaterial {
            id: mavi_core::CredentialId::new(),
            provider: "mail".to_owned(),
            name: "primary".to_owned(),
            version: 1,
            values: rotate.values.clone(),
        };

        assert!(!format!("{create:?}").contains("do-not-log-me"));
        assert!(!format!("{rotate:?}").contains("do-not-log-me"));
        assert!(!format!("{material:?}").contains("do-not-log-me"));
    }
}
