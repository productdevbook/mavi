use chrono::{DateTime, Utc};
use mavi_contract::{Endpoint, Method, Shape};
use mavi_core::{Email, ErrorCode, MailDeliveryId, MaviError, Result, SiteContext};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{MailService, lists::audit};

pub const MAIL_PROVIDER_EVENT_ID_INVALID: &str = "mail_provider_event_id_invalid";
pub const MAIL_PROVIDER_INVALID: &str = "mail_provider_invalid";
pub const MAIL_PROVIDER_REFERENCE_INVALID: &str = "mail_provider_reference_invalid";
pub const MAIL_PROVIDER_REASON_INVALID: &str = "mail_provider_reason_invalid";
pub const MAIL_PROVIDER_BOUNCE_CLASS_INVALID: &str = "mail_provider_bounce_class_invalid";
pub const MAIL_PROVIDER_EVENT_RECIPIENT_MISMATCH: &str = "mail_provider_event_recipient_mismatch";

const MAX_PROVIDER_CHARS: usize = 64;
const MAX_EVENT_ID_CHARS: usize = 256;
const MAX_REFERENCE_CHARS: usize = 1_024;
const MAX_REASON_CHARS: usize = 2_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MailProviderEventKind {
    Delivered,
    Bounced,
    Complained,
}

impl MailProviderEventKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Delivered => "delivered",
            Self::Bounced => "bounced",
            Self::Complained => "complained",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MailBounceClass {
    Transient,
    Permanent,
}

impl MailBounceClass {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transient => "transient",
            Self::Permanent => "permanent",
        }
    }
}

/// Provider gateways normalize vendor-specific callbacks before they submit
/// this site-scoped event. The gateway must preserve its own stable event ID
/// so retries are safe and a replay cannot repeatedly mutate suppression state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiveMailProviderEvent {
    pub provider: String,
    pub event_id: String,
    pub delivery_id: Option<MailDeliveryId>,
    pub recipient: String,
    pub kind: MailProviderEventKind,
    pub bounce_class: Option<MailBounceClass>,
    pub provider_reference: Option<String>,
    pub reason: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailProviderEventReceipt {
    pub duplicate: bool,
    pub suppressed: bool,
    pub cancelled_deliveries: u64,
}

pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new([Endpoint::new(
        Method::Post,
        "/internal/v1/mail/provider-events",
        "mail.provider_events.receive",
        "Receive one normalized provider delivery event",
    )
    .webhook_changes(true)
    .takes("ReceiveMailProviderEvent")
    .returns(200, "MailProviderEventReceipt")
    .refuses([
        ErrorCode::Unauthenticated,
        ErrorCode::Validation,
        ErrorCode::Conflict,
        ErrorCode::Internal,
    ])])
    .with_shapes([
        Shape::new(
            "MailProviderEventKind",
            json!({"type": "string", "enum": ["delivered", "bounced", "complained"]}),
        ),
        Shape::new(
            "MailBounceClass",
            json!({"type": "string", "enum": ["transient", "permanent"]}),
        ),
        Shape::new(
            "ReceiveMailProviderEvent",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["provider", "event_id", "recipient", "kind", "occurred_at"],
                "properties": {
                    "provider": {"type": "string", "minLength": 1, "maxLength": MAX_PROVIDER_CHARS},
                    "event_id": {"type": "string", "minLength": 1, "maxLength": MAX_EVENT_ID_CHARS},
                    "delivery_id": {"type": ["string", "null"], "format": "uuid"},
                    "recipient": {"type": "string", "format": "email"},
                    "kind": {"$ref": "#/components/schemas/MailProviderEventKind"},
                    "bounce_class": {"oneOf": [
                        {"$ref": "#/components/schemas/MailBounceClass"},
                        {"type": "null"}
                    ]},
                    "provider_reference": {"type": ["string", "null"], "maxLength": MAX_REFERENCE_CHARS},
                    "reason": {"type": ["string", "null"], "maxLength": MAX_REASON_CHARS},
                    "occurred_at": {"type": "string", "format": "date-time"}
                }
            }),
        ),
        Shape::new(
            "MailProviderEventReceipt",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["duplicate", "suppressed", "cancelled_deliveries"],
                "properties": {
                    "duplicate": {"type": "boolean"},
                    "suppressed": {"type": "boolean"},
                    "cancelled_deliveries": {"type": "integer", "format": "int64", "minimum": 0}
                }
            }),
        ),
    ])
}

impl MailService {
    pub async fn receive_provider_event(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &ReceiveMailProviderEvent,
    ) -> Result<MailProviderEventReceipt> {
        let provider = validate_text(&input.provider, MAX_PROVIDER_CHARS, MAIL_PROVIDER_INVALID)?;
        let event_id = validate_text(
            &input.event_id,
            MAX_EVENT_ID_CHARS,
            MAIL_PROVIDER_EVENT_ID_INVALID,
        )?;
        let email = Email::parse(&input.recipient)
            .map_err(|_| MaviError::validation_field("invalid_email", "recipient"))?;
        let provider_reference = input
            .provider_reference
            .as_deref()
            .map(|value| validate_text(value, MAX_REFERENCE_CHARS, MAIL_PROVIDER_REFERENCE_INVALID))
            .transpose()?;
        let reason = input
            .reason
            .as_deref()
            .map(|value| validate_text(value, MAX_REASON_CHARS, MAIL_PROVIDER_REASON_INVALID))
            .transpose()?;
        validate_bounce_class(input.kind, input.bounce_class)?;

        if let Some(delivery_id) = input.delivery_id {
            let stored_recipient = sqlx::query_scalar::<_, String>(
                "select recipient from mail_deliveries where site_id = $1 and id = $2",
            )
            .bind(context.site_id.into_uuid())
            .bind(delivery_id.into_uuid())
            .fetch_optional(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            if stored_recipient.is_some_and(|stored| stored != email.as_str()) {
                return Err(MaviError::conflict(MAIL_PROVIDER_EVENT_RECIPIENT_MISMATCH));
            }
        }

        let event_row_id = Uuid::now_v7();
        let inserted = sqlx::query_scalar::<_, Uuid>(
            "insert into mail_provider_events
                (site_id, id, provider, event_id, delivery_id, recipient, kind,
                 bounce_class, provider_reference, reason, occurred_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             on conflict (site_id, provider, event_id) do nothing
             returning id",
        )
        .bind(context.site_id.into_uuid())
        .bind(event_row_id)
        .bind(&provider)
        .bind(&event_id)
        .bind(input.delivery_id.map(MailDeliveryId::into_uuid))
        .bind(email.as_str())
        .bind(input.kind.as_str())
        .bind(input.bounce_class.map(MailBounceClass::as_str))
        .bind(provider_reference.as_deref())
        .bind(reason.as_deref())
        .bind(input.occurred_at)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if inserted.is_none() {
            return Ok(MailProviderEventReceipt {
                duplicate: true,
                suppressed: false,
                cancelled_deliveries: 0,
            });
        }

        let (suppressed, cancelled_deliveries) = match (input.kind, input.bounce_class) {
            (MailProviderEventKind::Bounced, Some(MailBounceClass::Permanent)) => {
                suppress_recipient(tx, context, email.as_str(), "mail_recipient_bounced").await?
            }
            (MailProviderEventKind::Complained, None) => {
                suppress_recipient(tx, context, email.as_str(), "mail_recipient_complained").await?
            }
            (MailProviderEventKind::Delivered, None)
            | (MailProviderEventKind::Bounced, Some(MailBounceClass::Transient)) => (false, 0),
            _ => return Err(MaviError::Internal),
        };

        audit(
            tx,
            context,
            "mail.provider_event.received",
            "MailProviderEvent",
            event_row_id,
            json!({
                "provider": provider,
                "event_id": event_id,
                "kind": input.kind,
                "bounce_class": input.bounce_class,
                "delivery_id": input.delivery_id,
                "suppressed": suppressed,
                "cancelled_deliveries": cancelled_deliveries,
            }),
        )
        .await?;

        Ok(MailProviderEventReceipt {
            duplicate: false,
            suppressed,
            cancelled_deliveries,
        })
    }
}

async fn suppress_recipient(
    tx: &mut SiteTx,
    context: &SiteContext,
    email: &str,
    cancellation_reason: &str,
) -> Result<(bool, u64)> {
    let standing = sqlx::query_scalar::<_, String>(
        "update mail_readers
            set standing = case when standing = 'complained' then standing else $3 end,
                updated_at = clock_timestamp()
          where site_id = $1 and email = $2 and deleted_at is null
         returning standing",
    )
    .bind(context.site_id.into_uuid())
    .bind(email)
    .bind(if cancellation_reason == "mail_recipient_complained" {
        "complained"
    } else {
        "bounced"
    })
    .fetch_optional(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    let cancelled = sqlx::query(
        "update mail_deliveries
            set status = 'cancelled', lease_owner = null, lease_until = null,
                last_error = $3, updated_at = clock_timestamp()
          where site_id = $1 and recipient = $2 and purpose = 'campaign'
            and status in ('queued', 'retry')",
    )
    .bind(context.site_id.into_uuid())
    .bind(email)
    .bind(cancellation_reason)
    .execute(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?
    .rows_affected();
    Ok((standing.is_some(), cancelled))
}

fn validate_bounce_class(
    kind: MailProviderEventKind,
    bounce_class: Option<MailBounceClass>,
) -> Result<()> {
    let valid = match kind {
        MailProviderEventKind::Bounced => bounce_class.is_some(),
        MailProviderEventKind::Delivered | MailProviderEventKind::Complained => {
            bounce_class.is_none()
        }
    };
    valid
        .then_some(())
        .ok_or_else(|| MaviError::validation(MAIL_PROVIDER_BOUNCE_CLASS_INVALID))
}

fn validate_text(value: &str, max_chars: usize, code: &'static str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max_chars || value.chars().any(char::is_control)
    {
        return Err(MaviError::validation(code));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use mavi_core::MailDeliveryId;

    use super::{
        MailBounceClass, MailProviderEventKind, ReceiveMailProviderEvent, validate_bounce_class,
        validate_text,
    };

    #[test]
    fn event_shape_requires_the_right_bounce_metadata() {
        assert!(validate_bounce_class(MailProviderEventKind::Bounced, None).is_err());
        assert!(validate_bounce_class(MailProviderEventKind::Complained, None).is_ok());
        assert!(
            validate_bounce_class(
                MailProviderEventKind::Bounced,
                Some(MailBounceClass::Permanent)
            )
            .is_ok()
        );
        assert!(
            validate_bounce_class(
                MailProviderEventKind::Delivered,
                Some(MailBounceClass::Transient)
            )
            .is_err()
        );
    }

    #[test]
    fn event_text_is_bounded_and_control_free() {
        assert_eq!(
            validate_text(" provider ", 32, "invalid").expect("text"),
            "provider"
        );
        assert!(validate_text("provider\nvalue", 32, "invalid").is_err());
    }

    #[test]
    fn event_payload_is_closed_and_typed() {
        let event = ReceiveMailProviderEvent {
            provider: "gateway".to_owned(),
            event_id: "evt-1".to_owned(),
            delivery_id: Some(MailDeliveryId::new()),
            recipient: "reader@example.test".to_owned(),
            kind: MailProviderEventKind::Bounced,
            bounce_class: Some(MailBounceClass::Permanent),
            provider_reference: Some("provider-1".to_owned()),
            reason: Some("mailbox missing".to_owned()),
            occurred_at: Utc::now(),
        };
        assert_eq!(event.kind.as_str(), "bounced");
        assert_eq!(
            event.bounce_class.map(MailBounceClass::as_str),
            Some("permanent")
        );
    }
}
