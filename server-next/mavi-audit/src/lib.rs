//! Immutable, site-scoped audit receipts for state-changing application services.
//!
//! Every mutation writes through [`AuditService::record`] in its existing
//! transaction. The same crate owns the read model and canonical API so audit
//! filters cannot drift away from the rows the application writes.

use std::collections::BTreeSet;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, AuditEventId, Caller, Capability, Cursor, ErrorCode, MaviError, Page, PageRequest,
    RequestId, Result, SiteContext, SiteId,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub const AUDIT_EVENT_NOT_FOUND: &str = "audit_event_not_found";
pub const AUDIT_FILTER_INVALID: &str = "audit_filter_invalid";
pub const AUDIT_RELOCATION_FORMAT: &str = "mavi.audit.relocation";
pub const AUDIT_RELOCATION_VERSION: u16 = 1;
pub const AUDIT_RELOCATION_CONFLICT: &str = "audit_relocation_conflict";
pub const MAX_RELOCATION_EVENTS: usize = 100_000;
pub const MAX_RELOCATION_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditActorKind {
    Public,
    Account,
    Student,
    Assistant,
    System,
}

impl AuditActorKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Account => "account",
            Self::Student => "student",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "public" => Ok(Self::Public),
            "account" => Ok(Self::Account),
            "student" => Ok(Self::Student),
            "assistant" => Ok(Self::Assistant),
            "system" => Ok(Self::System),
            _ => Err(MaviError::Internal),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AuditEntry {
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub payload: Value,
}

/// An immutable audit history carried by the trusted shard relocation port.
///
/// Public portable exports intentionally do not contain this history. The
/// source site ID is retained even though relocation keeps the logical site ID
/// unchanged, so a target cannot accept a bundle intended for another site.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuditRelocation {
    pub format: String,
    pub version: u16,
    pub source_site_id: SiteId,
    pub events: Vec<AuditRelocationEvent>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuditRelocationEvent {
    pub id: Uuid,
    pub request_id: Uuid,
    pub actor_kind: AuditActorKind,
    pub actor_id: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

impl AuditRelocation {
    #[must_use]
    pub fn empty(source_site_id: SiteId) -> Self {
        Self {
            format: AUDIT_RELOCATION_FORMAT.to_owned(),
            version: AUDIT_RELOCATION_VERSION,
            source_site_id,
            events: Vec::new(),
        }
    }

    pub fn validate_for_relocation(&self, target_site: SiteId) -> Result<()> {
        if self.format != AUDIT_RELOCATION_FORMAT {
            return Err(MaviError::validation("audit_relocation_format_invalid"));
        }
        if self.version != AUDIT_RELOCATION_VERSION {
            return Err(MaviError::validation(
                "audit_relocation_version_unsupported",
            ));
        }
        if self.source_site_id != target_site || self.source_site_id.into_uuid().is_nil() {
            return Err(MaviError::conflict("audit_relocation_site_mismatch"));
        }
        if self.events.len() > MAX_RELOCATION_EVENTS {
            return Err(MaviError::validation(
                "audit_relocation_event_count_invalid",
            ));
        }

        let mut ids = BTreeSet::new();
        for event in &self.events {
            if event.id.is_nil()
                || !ids.insert(event.id)
                || event.request_id.is_nil()
                || !valid_text(&event.action, 160)
                || !valid_text(&event.resource_type, 80)
                || event
                    .actor_id
                    .as_deref()
                    .is_some_and(|actor_id| !valid_text(actor_id, 255))
                || !event.payload.is_object()
            {
                return Err(MaviError::validation("audit_relocation_event_invalid"));
            }
        }

        let bytes = serde_json::to_vec(self).map_err(|_| MaviError::Internal)?;
        if bytes.len() > MAX_RELOCATION_BYTES {
            return Err(MaviError::validation("audit_relocation_too_large"));
        }
        Ok(())
    }

    pub fn record_count(&self) -> Result<i64> {
        i64::try_from(self.events.len())
            .map_err(|_| MaviError::validation("audit_relocation_event_count_overflow"))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AuditEvent {
    pub id: AuditEventId,
    pub request_id: RequestId,
    pub actor_kind: AuditActorKind,
    pub actor_id: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AuditListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<Uuid>,
    pub actor_kind: Option<AuditActorKind>,
    pub actor_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct AuditCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AuditService;

#[must_use]
pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new(endpoints()).with_shapes(shapes())
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/audit",
            "audit.events.list",
            "List immutable site audit events with an opaque cursor",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Audit,
            action: Action::View,
        })
        .takes_query("AuditListFilter")
        .returns(200, "AuditEventPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/audit/{id}",
            "audit.events.read",
            "Read one immutable audit event",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Audit,
            action: Action::View,
        })
        .returns(200, "AuditEvent")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
    ]
}

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "AuditActorKind",
            json!({"type": "string", "enum": ["public", "account", "student", "assistant", "system"]}),
        ),
        Shape::new(
            "AuditListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                    "action": {"type": ["string", "null"], "maxLength": 160},
                    "resource_type": {"type": ["string", "null"], "maxLength": 80},
                    "resource_id": {"type": ["string", "null"], "format": "uuid"},
                    "actor_kind": {"$ref": "#/components/schemas/AuditActorKind"},
                    "actor_id": {"type": ["string", "null"], "maxLength": 255},
                },
            }),
        ),
        Shape::new(
            "AuditEvent",
            json!({
                "type": "object",
                "required": ["id", "request_id", "actor_kind", "actor_id", "action", "resource_type", "resource_id", "payload", "created_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "request_id": {"type": "string", "format": "uuid"},
                    "actor_kind": {"$ref": "#/components/schemas/AuditActorKind"},
                    "actor_id": {"type": ["string", "null"], "maxLength": 255},
                    "action": {"type": "string", "maxLength": 160},
                    "resource_type": {"type": "string", "maxLength": 80},
                    "resource_id": {"type": ["string", "null"], "format": "uuid"},
                    "payload": {"type": "object", "additionalProperties": true},
                    "created_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "AuditEventPage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/AuditEvent"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512},
                },
            }),
        ),
    ]
}

impl AuditService {
    /// Writes the receipt in the caller's existing transaction. This must be
    /// called before the domain transaction commits.
    pub async fn record(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        entry: &AuditEntry,
    ) -> Result<()> {
        let (actor_kind, actor_id) = actor(context);
        sqlx::query(
            "insert into audit_events
                (site_id, id, request_id, actor_kind, actor_id, action, resource_type, resource_id, payload)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(context.site_id.into_uuid())
        .bind(Uuid::now_v7())
        .bind(context.request_id.into_uuid())
        .bind(actor_kind)
        .bind(actor_id)
        .bind(&entry.action)
        .bind(&entry.resource_type)
        .bind(entry.resource_id)
        .bind(&entry.payload)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        Ok(())
    }

    pub async fn list(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &AuditListFilter,
    ) -> Result<Page<AuditEvent>> {
        let action = validate_filter(filter.action.as_deref(), 160)?;
        let resource_type = validate_filter(filter.resource_type.as_deref(), 80)?;
        let actor_id = validate_filter(filter.actor_id.as_deref(), 255)?;
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, request_id, actor_kind, actor_id, action, resource_type,
                    resource_id, payload, created_at
               from audit_events where site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        if let Some(action) = action {
            query.push(" and action = ").push_bind(action);
        }
        if let Some(resource_type) = resource_type {
            query.push(" and resource_type = ").push_bind(resource_type);
        }
        if let Some(resource_id) = filter.resource_id {
            query.push(" and resource_id = ").push_bind(resource_id);
        }
        if let Some(actor_kind) = filter.actor_kind {
            query
                .push(" and actor_kind = ")
                .push_bind(actor_kind.as_str());
        }
        if let Some(actor_id) = actor_id {
            query.push(" and actor_id = ").push_bind(actor_id);
        }
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
        let mut items = rows.iter().map(from_row).collect::<Result<Vec<_>>>()?;
        let limit_usize = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit_usize {
            let last = items
                .get(limit_usize.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_cursor(last.created_at, last.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit_usize);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn get(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: AuditEventId,
    ) -> Result<AuditEvent> {
        let row = sqlx::query(
            "select id, request_id, actor_kind, actor_id, action, resource_type,
                    resource_id, payload, created_at
               from audit_events where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: AUDIT_EVENT_NOT_FOUND,
        })?;
        from_row(&row)
    }

    /// Exports immutable audit history for the trusted internal relocation
    /// port. The public audit API remains a read-only site-scoped view and does
    /// not expose this envelope type.
    pub async fn export_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
    ) -> Result<AuditRelocation> {
        let rows = sqlx::query(
            "select id, request_id, actor_kind, actor_id, action, resource_type,
                    resource_id, payload, created_at
               from audit_events
              where site_id = $1
              order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        let events = rows
            .iter()
            .map(|row| {
                Ok(AuditRelocationEvent {
                    id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                    request_id: row.try_get("request_id").map_err(|_| MaviError::Internal)?,
                    actor_kind: AuditActorKind::parse(
                        row.try_get::<String, _>("actor_kind")
                            .map_err(|_| MaviError::Internal)?
                            .as_str(),
                    )?,
                    actor_id: row.try_get("actor_id").map_err(|_| MaviError::Internal)?,
                    action: row.try_get("action").map_err(|_| MaviError::Internal)?,
                    resource_type: row
                        .try_get("resource_type")
                        .map_err(|_| MaviError::Internal)?,
                    resource_id: row
                        .try_get("resource_id")
                        .map_err(|_| MaviError::Internal)?,
                    payload: row.try_get("payload").map_err(|_| MaviError::Internal)?,
                    created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let relocation = AuditRelocation {
            format: AUDIT_RELOCATION_FORMAT.to_owned(),
            version: AUDIT_RELOCATION_VERSION,
            source_site_id: context.site_id,
            events,
        };
        relocation.validate_for_relocation(context.site_id)?;
        Ok(relocation)
    }

    /// Imports immutable event IDs idempotently and rejects a same-ID event
    /// whose contents differ. It deliberately emits no new audit event: the
    /// caller can record relocation activity separately without contaminating
    /// the source history being copied.
    pub async fn import_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        relocation: &AuditRelocation,
    ) -> Result<()> {
        relocation.validate_for_relocation(context.site_id)?;
        for event in &relocation.events {
            let inserted = sqlx::query(
                "insert into audit_events
                    (site_id, id, request_id, actor_kind, actor_id, action,
                     resource_type, resource_id, payload, created_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 on conflict (site_id, id) do nothing",
            )
            .bind(context.site_id.into_uuid())
            .bind(event.id)
            .bind(event.request_id)
            .bind(event.actor_kind.as_str())
            .bind(&event.actor_id)
            .bind(&event.action)
            .bind(&event.resource_type)
            .bind(event.resource_id)
            .bind(&event.payload)
            .bind(event.created_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            if inserted.rows_affected() == 0 {
                let same = sqlx::query(
                    "select request_id, actor_kind, actor_id, action, resource_type,
                            resource_id, payload, created_at
                       from audit_events
                      where site_id = $1 and id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(event.id)
                .fetch_optional(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?
                .is_some_and(|row| {
                    let actor_kind = row.try_get::<String, _>("actor_kind").ok();
                    let actor_id = row.try_get::<Option<String>, _>("actor_id").ok();
                    let request_id = row.try_get::<Uuid, _>("request_id").ok();
                    let action = row.try_get::<String, _>("action").ok();
                    let resource_type = row.try_get::<String, _>("resource_type").ok();
                    let resource_id = row.try_get::<Option<Uuid>, _>("resource_id").ok();
                    let payload = row.try_get::<Value, _>("payload").ok();
                    let created_at = row.try_get::<DateTime<Utc>, _>("created_at").ok();
                    request_id == Some(event.request_id)
                        && actor_kind.as_deref() == Some(event.actor_kind.as_str())
                        && actor_id.as_ref() == Some(&event.actor_id)
                        && action.as_deref() == Some(event.action.as_str())
                        && resource_type.as_deref() == Some(event.resource_type.as_str())
                        && resource_id == Some(event.resource_id)
                        && payload.as_ref() == Some(&event.payload)
                        && created_at == Some(event.created_at)
                });
                if !same {
                    return Err(MaviError::conflict(AUDIT_RELOCATION_CONFLICT));
                }
            }
        }
        Ok(())
    }
}

fn actor(context: &SiteContext) -> (&'static str, Option<String>) {
    match &context.caller {
        Caller::Public => (AuditActorKind::Public.as_str(), None),
        Caller::Account { person_id, .. } => (
            AuditActorKind::Account.as_str(),
            Some(person_id.to_string()),
        ),
        Caller::Student { student_id, .. } => (
            AuditActorKind::Student.as_str(),
            Some(student_id.to_string()),
        ),
        Caller::Assistant { key_id, .. } => {
            (AuditActorKind::Assistant.as_str(), Some(key_id.to_string()))
        }
        Caller::System { worker } => (AuditActorKind::System.as_str(), Some(worker.clone())),
    }
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<AuditEvent> {
    Ok(AuditEvent {
        id: AuditEventId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        request_id: RequestId::from_uuid(
            row.try_get("request_id").map_err(|_| MaviError::Internal)?,
        ),
        actor_kind: AuditActorKind::parse(
            row.try_get::<String, _>("actor_kind")
                .map_err(|_| MaviError::Internal)?
                .as_str(),
        )?,
        actor_id: row.try_get("actor_id").map_err(|_| MaviError::Internal)?,
        action: row.try_get("action").map_err(|_| MaviError::Internal)?,
        resource_type: row
            .try_get("resource_type")
            .map_err(|_| MaviError::Internal)?,
        resource_id: row
            .try_get("resource_id")
            .map_err(|_| MaviError::Internal)?,
        payload: row.try_get("payload").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
    })
}

fn validate_filter(value: Option<&str>, max_chars: usize) -> Result<Option<&str>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty() || value.chars().count() > max_chars {
        return Err(MaviError::validation(AUDIT_FILTER_INVALID));
    }
    Ok(Some(value))
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    !value.is_empty() && value.chars().count() <= max_chars && !value.chars().any(char::is_control)
}

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes =
        serde_json::to_vec(&AuditCursor { created_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor(cursor: &Cursor) -> Result<AuditCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_core::{ContentId, SiteId};

    #[test]
    fn public_audit_actor_is_not_fabricated_as_an_account() {
        let context = SiteContext::public(SiteId::new());
        assert_eq!(actor(&context), ("public", None));
    }

    #[test]
    fn content_id_can_be_recorded_as_a_uuid_resource() {
        let id = ContentId::new();
        let entry = AuditEntry {
            action: "content.created".to_owned(),
            resource_type: "Content".to_owned(),
            resource_id: Some(id.into_uuid()),
            payload: Value::Object(serde_json::Map::new()),
        };
        assert_eq!(entry.resource_id, Some(id.into_uuid()));
    }

    #[test]
    fn audit_cursor_and_contract_are_keyset_only() {
        let cursor = encode_cursor(Utc::now(), Uuid::now_v7()).expect("cursor");
        assert!(decode_cursor(&cursor).is_ok());
        assert!(decode_cursor(&Cursor::parse("bad").expect("cursor")).is_err());
        assert!(api().validate().is_ok());
        let filter = shapes()
            .into_iter()
            .find(|shape| shape.name == "AuditListFilter")
            .expect("audit filter");
        let properties = filter.schema["properties"].as_object().expect("properties");
        assert!(properties.contains_key("after"));
        assert!(properties.contains_key("limit"));
        assert!(!properties.contains_key("offset"));
        assert!(!properties.contains_key("page"));
    }

    #[test]
    fn relocation_is_site_bound_and_rejects_duplicate_event_ids() {
        let site = SiteId::new();
        let event = AuditRelocationEvent {
            id: Uuid::now_v7(),
            request_id: Uuid::now_v7(),
            actor_kind: AuditActorKind::Public,
            actor_id: None,
            action: "content.created".to_owned(),
            resource_type: "Content".to_owned(),
            resource_id: Some(Uuid::now_v7()),
            payload: json!({"ok": true}),
            created_at: Utc::now(),
        };
        let mut relocation = AuditRelocation {
            format: AUDIT_RELOCATION_FORMAT.to_owned(),
            version: AUDIT_RELOCATION_VERSION,
            source_site_id: site,
            events: vec![event.clone(), event],
        };
        assert!(relocation.validate_for_relocation(site).is_err());
        relocation.events.pop();
        assert!(relocation.validate_for_relocation(SiteId::new()).is_err());
        assert!(relocation.validate_for_relocation(site).is_ok());
    }
}
