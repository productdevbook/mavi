//! Immutable, site-scoped audit receipts for state-changing application services.
//!
//! Every mutation writes through [`AuditService::record`] in its existing
//! transaction. The same crate owns the read model and canonical API so audit
//! filters cannot drift away from the rows the application writes.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, AuditEventId, Caller, Capability, Cursor, ErrorCode, MaviError, Page, PageRequest,
    RequestId, Result, SiteContext,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub const AUDIT_EVENT_NOT_FOUND: &str = "audit_event_not_found";
pub const AUDIT_FILTER_INVALID: &str = "audit_filter_invalid";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditActorKind {
    Public,
    Account,
    Student,
    Assistant,
}

impl AuditActorKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Account => "account",
            Self::Student => "student",
            Self::Assistant => "assistant",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "public" => Ok(Self::Public),
            "account" => Ok(Self::Account),
            "student" => Ok(Self::Student),
            "assistant" => Ok(Self::Assistant),
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
            json!({"type": "string", "enum": ["public", "account", "student", "assistant"]}),
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
}
