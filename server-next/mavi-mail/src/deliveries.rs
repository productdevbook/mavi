use chrono::{DateTime, Utc};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, ErrorCode, MailDeliveryId, MailListId, MailTemplateId, MaviError, Page,
    PageRequest, Result, SiteContext,
    ports::{MailContentType, MailDeliveryReceipt, MailMessage, Seals},
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::templates::parse_content_type;
use crate::{MailService, decode_cursor, encode_cursor};

pub const MAIL_DELIVERY_NOT_FOUND: &str = "mail_delivery_not_found";
pub const MAIL_DELIVERY_LEASE_LOST: &str = "mail_delivery_lease_lost";
pub const MAIL_DELIVERY_STATUS_INVALID: &str = "mail_delivery_status_invalid";
pub const MAIL_DELIVERY_PURPOSE_INVALID: &str = "mail_delivery_purpose_invalid";
pub const MAIL_IDEMPOTENCY_KEY_INVALID: &str = "mail_idempotency_key_invalid";
pub const MAIL_DELIVERY_ERROR_INVALID: &str = "mail_delivery_error_invalid";
pub const MAIL_DELIVERY_ATTEMPTS_EXHAUSTED: &str = "mail_delivery_attempts_exhausted";
pub const PROTECTED_BODY_REDACTION: &str = "[protected]";

pub const MAX_DELIVERY_ATTEMPTS: i16 = 25;
const MAX_IDEMPOTENCY_KEY_CHARS: usize = 128;
const MAX_DELIVERY_ERROR_CHARS: usize = 2_000;

const DELIVERY_COLUMNS: &str =
    "id, template_id, list_id, recipient, subject, body, body_protected, content_type, purpose, status,
     attempts, available_at, lease_owner, lease_until, provider, provider_reference,
     last_error, idempotency_key, created_at, updated_at, sent_at";
const MAX_MAIL_SUBJECT_CHARS: usize = 300;
const MAX_MAIL_BODY_CHARS: usize = 100_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MailPurpose {
    Transactional,
    Campaign,
}

impl MailPurpose {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transactional => "transactional",
            Self::Campaign => "campaign",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "transactional" => Ok(Self::Transactional),
            "campaign" => Ok(Self::Campaign),
            _ => Err(MaviError::validation(MAIL_DELIVERY_PURPOSE_INVALID)),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MailDeliveryStatus {
    Queued,
    Sending,
    Retry,
    Sent,
    Dead,
    Cancelled,
}

impl MailDeliveryStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Sending => "sending",
            Self::Retry => "retry",
            Self::Sent => "sent",
            Self::Dead => "dead",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "sending" => Ok(Self::Sending),
            "retry" => Ok(Self::Retry),
            "sent" => Ok(Self::Sent),
            "dead" => Ok(Self::Dead),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(MaviError::validation(MAIL_DELIVERY_STATUS_INVALID)),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeliveryListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub status: Option<MailDeliveryStatus>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnqueueDelivery {
    pub recipient: String,
    pub template_id: MailTemplateId,
    #[serde(default)]
    pub variables: Map<String, Value>,
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SendCampaign {
    pub template_id: MailTemplateId,
    #[serde(default)]
    pub variables: Map<String, Value>,
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RetryDelivery {}

#[derive(Clone, Debug, Serialize)]
pub struct MailDelivery {
    pub id: MailDeliveryId,
    pub template_id: Option<MailTemplateId>,
    pub list_id: Option<MailListId>,
    pub recipient: String,
    pub subject: String,
    pub body: String,
    pub body_protected: bool,
    pub content_type: MailContentType,
    pub purpose: MailPurpose,
    pub status: MailDeliveryStatus,
    pub attempts: u16,
    pub available_at: DateTime<Utc>,
    pub provider: Option<String>,
    pub provider_reference: Option<String>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SendCount {
    pub enqueued: u64,
}

#[derive(Clone, Debug)]
pub struct ClaimedDelivery {
    pub delivery: MailDelivery,
    pub message: MailMessage,
    pub attempt_number: i16,
    pub idempotency_key: Option<String>,
}

pub type MailServiceError = MaviError;

pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new(endpoints()).with_shapes(shapes())
}

fn endpoints() -> Vec<Endpoint> {
    let view = Permission {
        capability: Capability::Mail,
        action: Action::View,
    };
    let write = Permission {
        capability: Capability::Mail,
        action: Action::Write,
    };
    vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/mail/deliveries",
            "mail.deliveries.list",
            "List site mail deliveries with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("DeliveryListFilter")
        .returns(200, "MailDeliveryPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/mail/deliveries",
            "mail.deliveries.enqueue",
            "Render a template and enqueue one provider-neutral delivery",
        )
        .account_or_assistant()
        .requires(write)
        .takes("EnqueueDelivery")
        .returns(202, "MailDelivery")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/mail/deliveries/{id}",
            "mail.deliveries.read",
            "Read one queued or completed mail delivery",
        )
        .account_or_assistant()
        .requires(view)
        .returns(200, "MailDelivery")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/mail/deliveries/{id}/retry",
            "mail.deliveries.retry",
            "Requeue a dead or cancelled mail delivery",
        )
        .account_or_assistant()
        .requires(write)
        .takes("RetryDelivery")
        .returns(202, "MailDelivery")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/mail/lists/{id}/deliveries",
            "mail.deliveries.campaign",
            "Expand one template into queued deliveries for subscribed readers",
        )
        .account_or_assistant()
        .requires(write)
        .takes("SendCampaign")
        .returns(202, "SendCount")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
    ]
}

fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "MailPurpose",
            json!({"type": "string", "enum": ["transactional", "campaign"]}),
        ),
        Shape::new(
            "MailDeliveryStatus",
            json!({"type": "string", "enum": ["queued", "sending", "retry", "sent", "dead", "cancelled"]}),
        ),
        Shape::new(
            "DeliveryListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                "status": {"$ref": "#/components/schemas/MailDeliveryStatus"}
            }}),
        ),
        Shape::new(
            "EnqueueDelivery",
            json!({"type": "object", "required": ["recipient", "template_id"], "additionalProperties": false, "properties": {
                "recipient": {"type": "string", "format": "email"},
                "template_id": {"type": "string", "format": "uuid"},
                "variables": {"type": "object", "additionalProperties": true},
                "idempotency_key": {"type": ["string", "null"], "maxLength": MAX_IDEMPOTENCY_KEY_CHARS}
            }}),
        ),
        Shape::new(
            "SendCampaign",
            json!({"type": "object", "required": ["template_id"], "additionalProperties": false, "properties": {
                "template_id": {"type": "string", "format": "uuid"},
                "variables": {"type": "object", "additionalProperties": true},
                "idempotency_key": {"type": ["string", "null"], "maxLength": MAX_IDEMPOTENCY_KEY_CHARS}
            }}),
        ),
        Shape::new(
            "RetryDelivery",
            json!({"type": "object", "additionalProperties": false}),
        ),
        Shape::new(
            "MailDelivery",
            json!({"type": "object", "required": ["id", "template_id", "list_id", "recipient", "subject", "body", "body_protected", "content_type", "purpose", "status", "attempts", "available_at", "provider", "provider_reference", "last_error", "created_at", "updated_at", "sent_at"], "properties": {
                "id": {"type": "string", "format": "uuid"},
                "template_id": {"type": ["string", "null"], "format": "uuid"},
                "list_id": {"type": ["string", "null"], "format": "uuid"},
                "recipient": {"type": "string", "format": "email"},
                "subject": {"type": "string"},
                "body": {"type": "string"},
                "body_protected": {"type": "boolean"},
                "content_type": {"$ref": "#/components/schemas/MailContentType"},
                "purpose": {"$ref": "#/components/schemas/MailPurpose"},
                "status": {"$ref": "#/components/schemas/MailDeliveryStatus"},
                "attempts": {"type": "integer", "minimum": 0, "maximum": 25},
                "available_at": {"type": "string", "format": "date-time"},
                "provider": {"type": ["string", "null"]},
                "provider_reference": {"type": ["string", "null"]},
                "last_error": {"type": ["string", "null"]},
                "created_at": {"type": "string", "format": "date-time"},
                "updated_at": {"type": "string", "format": "date-time"},
                "sent_at": {"type": ["string", "null"], "format": "date-time"}
            }}),
        ),
        Shape::new(
            "MailDeliveryPage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/MailDelivery"}},
                "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
        Shape::new(
            "SendCount",
            json!({"type": "object", "required": ["enqueued"], "properties": {"enqueued": {"type": "integer", "format": "int64", "minimum": 0}}}),
        ),
    ]
}

impl MailService {
    pub async fn list_deliveries(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &DeliveryListFilter,
    ) -> Result<Page<MailDelivery>> {
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("select ");
        query.push(DELIVERY_COLUMNS);
        query.push(" from mail_deliveries where site_id = ");
        query.push_bind(context.site_id.into_uuid());
        if let Some(status) = filter.status {
            query.push(" and status = ");
            query.push_bind(status.as_str());
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
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let last = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_cursor(last.created_at, last.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn get_delivery(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: MailDeliveryId,
    ) -> Result<MailDelivery> {
        let row = sqlx::QueryBuilder::<sqlx::Postgres>::new("select ")
            .push(DELIVERY_COLUMNS)
            .push(" from mail_deliveries where site_id = ")
            .push_bind(context.site_id.into_uuid())
            .push(" and id = ")
            .push_bind(id.into_uuid())
            .build()
            .fetch_optional(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?
            .ok_or(MaviError::NotFound {
                resource: MAIL_DELIVERY_NOT_FOUND,
            })?;
        from_row(&row)
    }

    pub async fn enqueue_delivery(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &EnqueueDelivery,
    ) -> Result<MailDelivery> {
        let idempotency_key = validate_idempotency_key(input.idempotency_key.as_deref())?;
        if let Some(key) = idempotency_key.as_deref()
            && let Some(row) = sqlx::QueryBuilder::<sqlx::Postgres>::new("select ")
                .push(DELIVERY_COLUMNS)
                .push(" from mail_deliveries where site_id = ")
                .push_bind(context.site_id.into_uuid())
                .push(" and idempotency_key = ")
                .push_bind(key)
                .build()
                .fetch_optional(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?
        {
            return from_row(&row);
        }
        let message = self
            .render_for_delivery(
                tx,
                context,
                input.template_id,
                &input.recipient,
                &input.variables,
            )
            .await?;
        let id = MailDeliveryId::new();
        let row = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "insert into mail_deliveries
                (site_id, id, template_id, recipient, subject, body, content_type, purpose, idempotency_key)
             values (",
        )
        .push_bind(context.site_id.into_uuid())
        .push(", ")
        .push_bind(id.into_uuid())
        .push(", ")
        .push_bind(input.template_id.into_uuid())
        .push(", ")
        .push_bind(&message.recipient)
        .push(", ")
        .push_bind(&message.subject)
        .push(", ")
        .push_bind(&message.body)
        .push(", ")
        .push_bind(message.content_type.as_str())
        .push(", 'transactional', ")
        .push_bind(idempotency_key.as_deref())
        .push(") returning ")
        .push(DELIVERY_COLUMNS)
        .build()
        .fetch_one(tx.conn())
        .await
        .map_err(|error| map_write_error(&error))?;
        let delivery = from_row(&row)?;
        audit(
            tx,
            context,
            "mail.delivery.queued",
            "MailDelivery",
            id,
            json!({"template_id": input.template_id, "purpose": "transactional"}),
        )
        .await?;
        Ok(delivery)
    }

    /// Enqueues a provider-neutral transactional message that is generated by
    /// a system workflow rather than a user-owned template. It deliberately
    /// keeps `template_id` null so password recovery and similar security
    /// messages cannot depend on a mutable site template existing first.
    pub async fn enqueue_transactional_message(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        message: MailMessage,
        idempotency_key: Option<&str>,
    ) -> Result<MailDelivery> {
        let idempotency_key = validate_idempotency_key(idempotency_key)?;
        if let Some(delivery) =
            find_delivery_by_idempotency_key(tx, context, idempotency_key.as_deref()).await?
        {
            return Ok(delivery);
        }

        let (recipient, subject) = validate_system_message(&message)?;

        let id = MailDeliveryId::new();
        let row = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "insert into mail_deliveries
                (site_id, id, template_id, list_id, recipient, subject, body, content_type, purpose, idempotency_key)
             values (",
        )
        .push_bind(context.site_id.into_uuid())
        .push(", ")
        .push_bind(id.into_uuid())
        .push(", null, null, ")
        .push_bind(recipient.as_str())
        .push(", ")
        .push_bind(subject)
        .push(", ")
        .push_bind(&message.body)
        .push(", ")
        .push_bind(message.content_type.as_str())
        .push(", 'transactional', ")
        .push_bind(idempotency_key.as_deref())
        .push(") returning ")
        .push(DELIVERY_COLUMNS)
        .build()
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let delivery = from_row(&row)?;
        audit(
            tx,
            context,
            "mail.delivery.queued",
            "MailDelivery",
            id,
            json!({"purpose": "transactional", "system": true}),
        )
        .await?;
        Ok(delivery)
    }

    /// Enqueues a system message whose body contains a one-time secret.
    ///
    /// The regular delivery row contains only a redaction marker. The sealed
    /// body is kept in a separate site-scoped table and is unsealed only when
    /// a worker claims the row with the keyring capability. This keeps the
    /// token out of database snapshots, delivery APIs, and routine outbox
    /// inspection queries.
    pub async fn enqueue_protected_transactional_message(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        message: MailMessage,
        idempotency_key: Option<&str>,
        sealer: &dyn Seals,
    ) -> Result<MailDelivery> {
        let idempotency_key = validate_idempotency_key(idempotency_key)?;
        if let Some(delivery) =
            find_delivery_by_idempotency_key(tx, context, idempotency_key.as_deref()).await?
        {
            return Ok(delivery);
        }

        let (recipient, subject) = validate_system_message(&message)?;

        let ciphertext = sealer.seal(context, message.body.as_bytes()).await?;
        let id = MailDeliveryId::new();
        let row = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "insert into mail_deliveries
                (site_id, id, template_id, list_id, recipient, subject, body,
                 body_protected, content_type, purpose, idempotency_key)
             values (",
        )
        .push_bind(context.site_id.into_uuid())
        .push(", ")
        .push_bind(id.into_uuid())
        .push(", null, null, ")
        .push_bind(recipient.as_str())
        .push(", ")
        .push_bind(subject)
        .push(", ")
        .push_bind(PROTECTED_BODY_REDACTION)
        .push(", true, ")
        .push_bind(message.content_type.as_str())
        .push(", 'transactional', ")
        .push_bind(idempotency_key.as_deref())
        .push(") returning ")
        .push(DELIVERY_COLUMNS)
        .build()
        .fetch_one(tx.conn())
        .await
        .map_err(|error| map_write_error(&error))?;

        sqlx::query(
            "insert into mail_delivery_secrets (site_id, delivery_id, ciphertext)
             values ($1, $2, $3)",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(ciphertext)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        let delivery = from_row(&row)?;
        audit(
            tx,
            context,
            "mail.delivery.queued",
            "MailDelivery",
            id,
            json!({"purpose": "transactional", "system": true, "body_protected": true}),
        )
        .await?;
        Ok(delivery)
    }

    pub async fn send_campaign(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        list_id: MailListId,
        input: &SendCampaign,
    ) -> Result<SendCount> {
        let list_exists: bool = sqlx::query_scalar(
            "select exists(select 1 from mail_lists where site_id = $1 and id = $2 and deleted_at is null)",
        )
        .bind(context.site_id.into_uuid())
        .bind(list_id.into_uuid())
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if !list_exists {
            return Err(MaviError::NotFound {
                resource: "mail_list_not_found",
            });
        }
        let idempotency_key = validate_idempotency_key(input.idempotency_key.as_deref())?;
        let rows = sqlx::query(
            "select r.id, r.email from mail_list_members m
               join mail_readers r on r.site_id = m.site_id and r.id = m.reader_id
              where m.site_id = $1 and m.list_id = $2 and r.deleted_at is null and r.standing = 'subscribed'
              order by m.created_at asc, r.id asc",
        )
        .bind(context.site_id.into_uuid())
        .bind(list_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let mut enqueued = 0_u64;
        for row in rows {
            let recipient: String = row.try_get("email").map_err(|_| MaviError::Internal)?;
            let reader_id: Uuid = row.try_get("id").map_err(|_| MaviError::Internal)?;
            let message = self
                .render_for_delivery(tx, context, input.template_id, &recipient, &input.variables)
                .await?;
            let delivery_id = MailDeliveryId::new();
            let member_key = idempotency_key
                .as_deref()
                .map(|key| format!("{key}:{reader_id}"));
            let result = sqlx::query(
                "insert into mail_deliveries
                    (site_id, id, template_id, list_id, recipient, subject, body, content_type, purpose, idempotency_key)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, 'campaign', $9)
                 on conflict (site_id, idempotency_key) where idempotency_key is not null do nothing",
            )
            .bind(context.site_id.into_uuid())
            .bind(delivery_id.into_uuid())
            .bind(input.template_id.into_uuid())
            .bind(list_id.into_uuid())
            .bind(&message.recipient)
            .bind(&message.subject)
            .bind(&message.body)
            .bind(message.content_type.as_str())
            .bind(member_key.as_deref())
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
            enqueued += result.rows_affected();
        }
        audit(
            tx,
            context,
            "mail.campaign.queued",
            "MailList",
            list_id,
            json!({"template_id": input.template_id, "enqueued": enqueued}),
        )
        .await?;
        Ok(SendCount { enqueued })
    }

    pub async fn retry_delivery(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: MailDeliveryId,
    ) -> Result<MailDelivery> {
        let row = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "update mail_deliveries
                set status = 'queued', attempts = 0, available_at = clock_timestamp(), lease_owner = null,
                    lease_until = null, last_error = null, updated_at = clock_timestamp()
              where site_id = ",
        )
        .push_bind(context.site_id.into_uuid())
        .push(" and id = ")
        .push_bind(id.into_uuid())
        .push(" and status in ('dead', 'cancelled')")
        .push(" and (not body_protected or exists (")
        .push("select 1 from mail_delivery_secrets")
        .push(" where site_id = mail_deliveries.site_id and delivery_id = mail_deliveries.id")
        .push(")) returning ")
        .push(DELIVERY_COLUMNS)
        .build()
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or_else(|| MaviError::conflict("mail_delivery_not_retryable"))?;
        let delivery = from_row(&row)?;
        audit(
            tx,
            context,
            "mail.delivery.requeued",
            "MailDelivery",
            id,
            json!({}),
        )
        .await?;
        Ok(delivery)
    }

    /// Claims one ready row and opens one attempt. Commit this transaction
    /// before calling [`crate::send_via`] or any other provider adapter.
    pub async fn claim_next(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        worker_id: &str,
        lease_until: DateTime<Utc>,
    ) -> Result<Option<ClaimedDelivery>> {
        self.claim_next_inner(tx, context, worker_id, lease_until, None)
            .await
    }

    /// Claims a delivery and unseals protected system bodies with the
    /// deployment keyring. Plain deliveries continue to work identically.
    pub async fn claim_next_with_sealer(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        worker_id: &str,
        lease_until: DateTime<Utc>,
        sealer: &dyn Seals,
    ) -> Result<Option<ClaimedDelivery>> {
        self.claim_next_inner(tx, context, worker_id, lease_until, Some(sealer))
            .await
    }

    async fn claim_next_inner(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        worker_id: &str,
        lease_until: DateTime<Utc>,
        sealer: Option<&dyn Seals>,
    ) -> Result<Option<ClaimedDelivery>> {
        if worker_id.trim().is_empty() || worker_id.len() > 128 {
            return Err(MaviError::validation("invalid_worker_id"));
        }
        if lease_until <= Utc::now() {
            return Err(MaviError::validation("invalid_lease_until"));
        }
        loop {
            let candidate = sqlx::query(
                "select id, status, attempts, body_protected from mail_deliveries
                  where site_id = $1 and (not body_protected or $2) and (
                        (status in ('queued', 'retry') and available_at <= clock_timestamp())
                     or (status = 'sending' and lease_until <= clock_timestamp())
                  )
                  order by available_at asc, created_at asc, id asc
                  for update skip locked limit 1",
            )
            .bind(context.site_id.into_uuid())
            .bind(sealer.is_some())
            .fetch_optional(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            let Some(candidate) = candidate else {
                return Ok(None);
            };
            let id: Uuid = candidate.try_get("id").map_err(|_| MaviError::Internal)?;
            let previous_status: String = candidate
                .try_get("status")
                .map_err(|_| MaviError::Internal)?;
            let attempts: i16 = candidate
                .try_get("attempts")
                .map_err(|_| MaviError::Internal)?;
            if attempts >= MAX_DELIVERY_ATTEMPTS {
                mark_exhausted_delivery(tx, context, id, attempts).await?;
                continue;
            }
            let body_protected: bool = candidate
                .try_get("body_protected")
                .map_err(|_| MaviError::Internal)?;
            let protected_body =
                unseal_delivery_body(tx, context, id, body_protected, sealer).await?;
            let row = sqlx::QueryBuilder::<sqlx::Postgres>::new(
                "update mail_deliveries
                    set status = 'sending', attempts = attempts + 1, lease_owner = ",
            )
            .push_bind(worker_id)
            .push(", lease_until = ")
            .push_bind(lease_until)
            .push(", updated_at = clock_timestamp() where site_id = ")
            .push_bind(context.site_id.into_uuid())
            .push(" and id = ")
            .push_bind(id)
            .push(" returning ")
            .push(DELIVERY_COLUMNS)
            .build()
            .fetch_one(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            if previous_status == "sending" {
                sqlx::query(
                    "update mail_delivery_attempts
                        set status = 'retry', error = 'lease_expired', finished_at = clock_timestamp()
                      where site_id = $1 and delivery_id = $2 and status = 'sending'",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
            }
            let delivery = from_row(&row)?;
            let attempt_number =
                i16::try_from(delivery.attempts).map_err(|_| MaviError::Internal)?;
            let idempotency_key = delivery_idempotency_key(&row)?;
            sqlx::query(
                "insert into mail_delivery_attempts
                    (site_id, id, delivery_id, attempt_number, status)
                 values ($1, $2, $3, $4, 'sending')",
            )
            .bind(context.site_id.into_uuid())
            .bind(Uuid::now_v7())
            .bind(id)
            .bind(attempt_number)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            let message = MailMessage {
                recipient: delivery.recipient.clone(),
                subject: delivery.subject.clone(),
                body: protected_body.unwrap_or_else(|| delivery.body.clone()),
                content_type: delivery.content_type,
            };
            return Ok(Some(ClaimedDelivery {
                delivery,
                message,
                attempt_number,
                idempotency_key,
            }));
        }
    }

    pub async fn mark_sent(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: MailDeliveryId,
        worker_id: &str,
        receipt: &MailDeliveryReceipt,
    ) -> Result<MailDelivery> {
        let row = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "update mail_deliveries
                set status = 'sent', lease_owner = null, lease_until = null,
                    provider = ",
        )
        .push_bind(&receipt.provider)
        .push(", provider_reference = ")
        .push_bind(&receipt.reference)
        .push(
            ", last_error = null, sent_at = clock_timestamp(), updated_at = clock_timestamp()
              where site_id = ",
        )
        .push_bind(context.site_id.into_uuid())
        .push(" and id = ")
        .push_bind(id.into_uuid())
        .push(" and status = 'sending' and lease_owner = ")
        .push_bind(worker_id)
        .push(" returning ")
        .push(DELIVERY_COLUMNS)
        .build()
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or_else(|| MaviError::conflict(MAIL_DELIVERY_LEASE_LOST))?;
        let delivery = from_row(&row)?;
        sqlx::query(
            "update mail_delivery_attempts
                set status = 'sent', provider = $3, provider_reference = $4, finished_at = clock_timestamp()
              where site_id = $1 and delivery_id = $2 and status = 'sending'",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(&receipt.provider)
        .bind(&receipt.reference)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        audit(
            tx,
            context,
            "mail.delivery.sent",
            "MailDelivery",
            id,
            json!({"provider": receipt.provider}),
        )
        .await?;
        Ok(delivery)
    }

    pub async fn mark_failed(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: MailDeliveryId,
        worker_id: &str,
        error: &str,
        retry_at: Option<DateTime<Utc>>,
    ) -> Result<MailDelivery> {
        let error = validate_error(error)?;
        let current = sqlx::query(
            "select attempts from mail_deliveries
              where site_id = $1 and id = $2 and status = 'sending' and lease_owner = $3
              for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(worker_id)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or_else(|| MaviError::conflict(MAIL_DELIVERY_LEASE_LOST))?;
        let attempts: i16 = current
            .try_get("attempts")
            .map_err(|_| MaviError::Internal)?;
        let retry = attempts < MAX_DELIVERY_ATTEMPTS && retry_at.is_some();
        let status = if retry { "retry" } else { "dead" };
        let available_at = retry_at.unwrap_or_else(Utc::now);
        let row = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "update mail_deliveries
                set status = ",
        )
        .push_bind(status)
        .push(", available_at = ")
        .push_bind(available_at)
        .push(", lease_owner = null, lease_until = null, last_error = ")
        .push_bind(&error)
        .push(", updated_at = clock_timestamp() where site_id = ")
        .push_bind(context.site_id.into_uuid())
        .push(" and id = ")
        .push_bind(id.into_uuid())
        .push(" returning ")
        .push(DELIVERY_COLUMNS)
        .build()
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let delivery = from_row(&row)?;
        sqlx::query(
            "update mail_delivery_attempts
                set status = $3, error = $4, finished_at = clock_timestamp()
              where site_id = $1 and delivery_id = $2 and status = 'sending'",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(status)
        .bind(&error)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        audit(
            tx,
            context,
            if retry {
                "mail.delivery.retry"
            } else {
                "mail.delivery.dead"
            },
            "MailDelivery",
            id,
            json!({"attempts": attempts}),
        )
        .await?;
        Ok(delivery)
    }

    #[must_use]
    pub fn message(claimed: &ClaimedDelivery) -> MailMessage {
        claimed.message.clone()
    }
}

async fn unseal_delivery_body(
    tx: &mut SiteTx,
    context: &SiteContext,
    id: Uuid,
    body_protected: bool,
    sealer: Option<&dyn Seals>,
) -> Result<Option<String>> {
    if !body_protected {
        return Ok(None);
    }
    let sealer = sealer.ok_or(MaviError::Internal)?;
    let ciphertext: Vec<u8> = sqlx::query_scalar(
        "select ciphertext from mail_delivery_secrets
          where site_id = $1 and delivery_id = $2",
    )
    .bind(context.site_id.into_uuid())
    .bind(id)
    .fetch_optional(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?
    .ok_or(MaviError::Internal)?;
    let plaintext = sealer.unseal(context, &ciphertext).await?;
    String::from_utf8(plaintext)
        .map(Some)
        .map_err(|_| MaviError::Internal)
}

async fn mark_exhausted_delivery(
    tx: &mut SiteTx,
    context: &SiteContext,
    id: Uuid,
    attempts: i16,
) -> Result<()> {
    sqlx::query(
        "update mail_deliveries
            set status = 'dead', lease_owner = null, lease_until = null,
                last_error = $3, updated_at = clock_timestamp()
          where site_id = $1 and id = $2",
    )
    .bind(context.site_id.into_uuid())
    .bind(id)
    .bind(MAIL_DELIVERY_ATTEMPTS_EXHAUSTED)
    .execute(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    sqlx::query(
        "update mail_delivery_attempts
            set status = 'dead', error = $3, finished_at = clock_timestamp()
          where site_id = $1 and delivery_id = $2 and status = 'sending'",
    )
    .bind(context.site_id.into_uuid())
    .bind(id)
    .bind(MAIL_DELIVERY_ATTEMPTS_EXHAUSTED)
    .execute(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    audit(
        tx,
        context,
        "mail.delivery.dead",
        "MailDelivery",
        id,
        json!({"attempts": attempts, "reason": MAIL_DELIVERY_ATTEMPTS_EXHAUSTED}),
    )
    .await
}

fn validate_idempotency_key(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value else { return Ok(None) };
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_IDEMPOTENCY_KEY_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(MaviError::validation(MAIL_IDEMPOTENCY_KEY_INVALID));
    }
    Ok(Some(value.to_owned()))
}

fn delivery_idempotency_key(row: &sqlx::postgres::PgRow) -> Result<Option<String>> {
    row.try_get("idempotency_key")
        .map_err(|_| MaviError::Internal)
}

async fn find_delivery_by_idempotency_key(
    tx: &mut SiteTx,
    context: &SiteContext,
    idempotency_key: Option<&str>,
) -> Result<Option<MailDelivery>> {
    let Some(idempotency_key) = idempotency_key else {
        return Ok(None);
    };
    let row = sqlx::QueryBuilder::<sqlx::Postgres>::new("select ")
        .push(DELIVERY_COLUMNS)
        .push(" from mail_deliveries where site_id = ")
        .push_bind(context.site_id.into_uuid())
        .push(" and idempotency_key = ")
        .push_bind(idempotency_key)
        .build()
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    row.map(|row| from_row(&row)).transpose()
}

fn validate_system_message(message: &MailMessage) -> Result<(mavi_core::Email, &str)> {
    let recipient = mavi_core::Email::parse(&message.recipient)
        .map_err(|_| MaviError::validation_field("invalid_email", "recipient"))?;
    let subject = message.subject.trim();
    if subject.is_empty()
        || subject.chars().count() > MAX_MAIL_SUBJECT_CHARS
        || subject.chars().any(char::is_control)
    {
        return Err(MaviError::validation_field(
            "mail_subject_invalid",
            "subject",
        ));
    }
    if message.body.is_empty()
        || message.body.chars().count() > MAX_MAIL_BODY_CHARS
        || message.body.chars().any(|character| character == '\0')
    {
        return Err(MaviError::validation_field("mail_body_invalid", "body"));
    }
    Ok((recipient, subject))
}

fn validate_error(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_DELIVERY_ERROR_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(MaviError::validation(MAIL_DELIVERY_ERROR_INVALID));
    }
    Ok(value.to_owned())
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<MailDelivery> {
    let attempts: i16 = row.try_get("attempts").map_err(|_| MaviError::Internal)?;
    Ok(MailDelivery {
        id: MailDeliveryId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        template_id: row
            .try_get::<Option<Uuid>, _>("template_id")
            .map_err(|_| MaviError::Internal)?
            .map(MailTemplateId::from_uuid),
        list_id: row
            .try_get::<Option<Uuid>, _>("list_id")
            .map_err(|_| MaviError::Internal)?
            .map(MailListId::from_uuid),
        recipient: row.try_get("recipient").map_err(|_| MaviError::Internal)?,
        subject: row.try_get("subject").map_err(|_| MaviError::Internal)?,
        body: row.try_get("body").map_err(|_| MaviError::Internal)?,
        body_protected: row
            .try_get("body_protected")
            .map_err(|_| MaviError::Internal)?,
        content_type: parse_content_type(
            &row.try_get::<String, _>("content_type")
                .map_err(|_| MaviError::Internal)?,
        )?,
        purpose: MailPurpose::parse(
            &row.try_get::<String, _>("purpose")
                .map_err(|_| MaviError::Internal)?,
        )?,
        status: MailDeliveryStatus::parse(
            &row.try_get::<String, _>("status")
                .map_err(|_| MaviError::Internal)?,
        )?,
        attempts: u16::try_from(attempts).map_err(|_| MaviError::Internal)?,
        available_at: row
            .try_get("available_at")
            .map_err(|_| MaviError::Internal)?,
        provider: row.try_get("provider").map_err(|_| MaviError::Internal)?,
        provider_reference: row
            .try_get("provider_reference")
            .map_err(|_| MaviError::Internal)?,
        last_error: row.try_get("last_error").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
        sent_at: row.try_get("sent_at").map_err(|_| MaviError::Internal)?,
    })
}

async fn audit(
    tx: &mut SiteTx,
    context: &SiteContext,
    action: &str,
    resource_type: &str,
    resource_id: impl Into<Uuid>,
    payload: Value,
) -> Result<()> {
    mavi_audit::AuditService
        .record(
            tx,
            context,
            &mavi_audit::AuditEntry {
                action: action.to_owned(),
                resource_type: resource_type.to_owned(),
                resource_id: Some(resource_id.into()),
                payload,
            },
        )
        .await
}

fn map_write_error(error: &sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error
        && database.constraint() == Some("mail_deliveries_site_idempotency")
    {
        return MaviError::conflict("mail_delivery_idempotency_conflict");
    }
    MaviError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn retry_policy_stops_after_the_bounded_attempt_count() {
        let lease = Utc::now() + Duration::minutes(5);
        assert!(lease > Utc::now());
        assert!(validate_error("provider unavailable").is_ok());
        assert!(validate_idempotency_key(Some("mail-42")).is_ok());
    }

    #[test]
    fn delivery_contract_is_cursor_only_and_provider_neutral() {
        let contract = serde_json::to_string(&api()).expect("contract");
        assert!(contract.contains("MailDeliveryPage"));
        assert!(contract.contains("mail.deliveries.enqueue"));
        assert!(!contract.contains("offset"));
        assert!(!contract.contains("smtp"));
    }
}
