use std::collections::BTreeSet;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_core::{Email, MaviError, Result, SiteContext, SiteId};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use super::{MailService, PROTECTED_BODY_REDACTION};

pub const MAIL_RELOCATION_FORMAT: &str = "mavi.mail.relocation";
pub const MAIL_RELOCATION_VERSION: u16 = 1;
pub const MAX_MAIL_RELOCATION_RECORDS: usize = 100_000;
pub const MAX_MAIL_RELOCATION_BYTES: usize = 256 * 1024 * 1024;

/// Authenticated shard relocation data for mail configuration and outbox
/// state. Provider credentials are deliberately absent; they move through the
/// separate credentials capability. A source `sending` lease is represented
/// as a retryable delivery so a target worker can safely resume it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MailRelocation {
    pub format: String,
    pub version: u16,
    pub source_site_id: SiteId,
    pub templates: Vec<MailTemplateRelocation>,
    pub lists: Vec<MailListRelocation>,
    pub readers: Vec<MailReaderRelocation>,
    pub memberships: Vec<MailListMemberRelocation>,
    pub deliveries: Vec<MailDeliveryRelocation>,
    pub attempts: Vec<MailDeliveryAttemptRelocation>,
    pub unsubscribe_tokens: Vec<MailUnsubscribeTokenRelocation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MailTemplateRelocation {
    pub id: Uuid,
    pub key: String,
    pub language: String,
    pub subject: String,
    pub body: String,
    pub content_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MailListRelocation {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MailReaderRelocation {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub standing: String,
    #[serde(with = "base64_bytes")]
    pub unsubscribe_token_hash: Vec<u8>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MailListMemberRelocation {
    pub list_id: Uuid,
    pub reader_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MailDeliveryRelocation {
    pub id: Uuid,
    pub template_id: Option<Uuid>,
    pub list_id: Option<Uuid>,
    pub recipient: String,
    pub subject: String,
    pub body: String,
    /// Protected bodies are intentionally relocated without their ciphertext
    /// so a stale token cannot become sendable on the target runtime.
    #[serde(default)]
    pub body_protected: bool,
    pub content_type: String,
    pub purpose: String,
    pub status: String,
    pub attempts: i16,
    pub available_at: DateTime<Utc>,
    pub provider: Option<String>,
    pub provider_reference: Option<String>,
    pub last_error: Option<String>,
    pub idempotency_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MailDeliveryAttemptRelocation {
    pub id: Uuid,
    pub delivery_id: Uuid,
    pub attempt_number: i16,
    pub status: String,
    pub provider: Option<String>,
    pub provider_reference: Option<String>,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MailUnsubscribeTokenRelocation {
    pub id: Uuid,
    pub delivery_id: Uuid,
    pub reader_id: Uuid,
    #[serde(with = "base64_bytes")]
    pub token_hash: Vec<u8>,
    pub created_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
}

impl MailRelocation {
    #[must_use]
    pub fn empty(source_site_id: SiteId) -> Self {
        Self {
            format: MAIL_RELOCATION_FORMAT.to_owned(),
            version: MAIL_RELOCATION_VERSION,
            source_site_id,
            templates: Vec::new(),
            lists: Vec::new(),
            readers: Vec::new(),
            memberships: Vec::new(),
            deliveries: Vec::new(),
            attempts: Vec::new(),
            unsubscribe_tokens: Vec::new(),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn validate_for_relocation(&self, target_site: SiteId) -> Result<()> {
        if self.format != MAIL_RELOCATION_FORMAT {
            return Err(MaviError::validation("mail_relocation_format_invalid"));
        }
        if self.version != MAIL_RELOCATION_VERSION {
            return Err(MaviError::validation("mail_relocation_version_unsupported"));
        }
        if self.source_site_id != target_site || self.source_site_id.into_uuid().is_nil() {
            return Err(MaviError::conflict("mail_relocation_site_mismatch"));
        }
        let counts = [
            self.templates.len(),
            self.lists.len(),
            self.readers.len(),
            self.memberships.len(),
            self.deliveries.len(),
            self.attempts.len(),
            self.unsubscribe_tokens.len(),
        ];
        if counts
            .iter()
            .try_fold(0usize, |total, count| total.checked_add(*count))
            .is_none_or(|count| count > MAX_MAIL_RELOCATION_RECORDS)
        {
            return Err(MaviError::validation("mail_relocation_counts_invalid"));
        }

        let mut template_ids = BTreeSet::new();
        let mut template_keys = BTreeSet::new();
        for template in &self.templates {
            if template.id.is_nil()
                || !template_ids.insert(template.id)
                || !valid_template_key(&template.key)
                || !valid_language(&template.language)
                || !valid_subject(&template.subject)
                || !valid_body(&template.body)
                || !valid_content_type(&template.content_type)
                || (template.deleted_at.is_none()
                    && !template_keys.insert((template.key.as_str(), template.language.as_str())))
            {
                return Err(MaviError::validation("mail_relocation_template_invalid"));
            }
        }

        let mut list_ids = BTreeSet::new();
        let mut list_slugs = BTreeSet::new();
        for list in &self.lists {
            if list.id.is_nil()
                || !list_ids.insert(list.id)
                || !valid_list_slug(&list.slug)
                || !valid_name(&list.name, 200)
                || !list_slugs.insert(list.slug.as_str())
            {
                return Err(MaviError::validation("mail_relocation_list_invalid"));
            }
        }

        let mut reader_ids = BTreeSet::new();
        let mut reader_emails = BTreeSet::new();
        let mut reader_tokens = BTreeSet::new();
        for reader in &self.readers {
            if reader.id.is_nil()
                || !reader_ids.insert(reader.id)
                || !valid_email(&reader.email)
                || (reader.deleted_at.is_none() && !reader_emails.insert(reader.email.as_str()))
                || (reader.deleted_at.is_none()
                    && !reader_tokens.insert(reader.unsubscribe_token_hash.as_slice()))
                || reader
                    .name
                    .as_deref()
                    .is_some_and(|name| !valid_name(name, 200))
                || !matches!(
                    reader.standing.as_str(),
                    "subscribed" | "unsubscribed" | "bounced" | "complained"
                )
                || reader.unsubscribe_token_hash.len() != 32
            {
                return Err(MaviError::validation("mail_relocation_reader_invalid"));
            }
        }

        let mut memberships = BTreeSet::new();
        for membership in &self.memberships {
            if !list_ids.contains(&membership.list_id)
                || !reader_ids.contains(&membership.reader_id)
                || !memberships.insert((membership.list_id, membership.reader_id))
            {
                return Err(MaviError::validation("mail_relocation_membership_invalid"));
            }
        }

        let mut delivery_ids = BTreeSet::new();
        let mut idempotency_keys = BTreeSet::new();
        for delivery in &self.deliveries {
            if delivery.id.is_nil()
                || !delivery_ids.insert(delivery.id)
                || delivery
                    .template_id
                    .is_some_and(|id| !template_ids.contains(&id))
                || delivery.list_id.is_some_and(|id| !list_ids.contains(&id))
                || !valid_email(&delivery.recipient)
                || !valid_subject(&delivery.subject)
                || !valid_body(&delivery.body)
                || (delivery.body_protected && delivery.body != PROTECTED_BODY_REDACTION)
                || !valid_content_type(&delivery.content_type)
                || !matches!(delivery.purpose.as_str(), "transactional" | "campaign")
                || !matches!(
                    delivery.status.as_str(),
                    "queued" | "retry" | "sent" | "dead" | "cancelled"
                )
                || !(0..=25).contains(&delivery.attempts)
                || !valid_optional_text(delivery.provider.as_deref(), 255)
                || !valid_optional_text(delivery.provider_reference.as_deref(), 1024)
                || !valid_optional_text(delivery.last_error.as_deref(), 2_000)
                || !valid_optional_key(delivery.idempotency_key.as_deref(), 128)
                || delivery
                    .idempotency_key
                    .as_deref()
                    .is_some_and(|key| !idempotency_keys.insert(key))
            {
                return Err(MaviError::validation("mail_relocation_delivery_invalid"));
            }
        }

        let mut attempt_ids = BTreeSet::new();
        let mut delivery_attempts = BTreeSet::new();
        for attempt in &self.attempts {
            if attempt.id.is_nil()
                || !attempt_ids.insert(attempt.id)
                || !delivery_ids.contains(&attempt.delivery_id)
                || !(1..=25).contains(&attempt.attempt_number)
                || !matches!(attempt.status.as_str(), "sent" | "retry" | "dead")
                || attempt.finished_at.is_none()
                || !valid_optional_text(attempt.provider.as_deref(), 255)
                || !valid_optional_text(attempt.provider_reference.as_deref(), 1024)
                || !valid_optional_text(attempt.error.as_deref(), 2_000)
                || !delivery_attempts.insert((attempt.delivery_id, attempt.attempt_number))
            {
                return Err(MaviError::validation("mail_relocation_attempt_invalid"));
            }
        }

        let mut unsubscribe_token_ids = BTreeSet::new();
        let mut unsubscribe_hashes = BTreeSet::new();
        let mut unsubscribe_deliveries = BTreeSet::new();
        for token in &self.unsubscribe_tokens {
            if token.id.is_nil()
                || !unsubscribe_token_ids.insert(token.id)
                || !delivery_ids.contains(&token.delivery_id)
                || !reader_ids.contains(&token.reader_id)
                || token.token_hash.len() != 32
                || !unsubscribe_hashes.insert(token.token_hash.as_slice())
                || !unsubscribe_deliveries.insert(token.delivery_id)
            {
                return Err(MaviError::validation(
                    "mail_relocation_unsubscribe_token_invalid",
                ));
            }
        }

        let bytes = serde_json::to_vec(self).map_err(|_| MaviError::Internal)?;
        if bytes.len() > MAX_MAIL_RELOCATION_BYTES {
            return Err(MaviError::validation("mail_relocation_too_large"));
        }
        Ok(())
    }

    pub fn record_count(&self) -> Result<i64> {
        let count = [
            self.templates.len(),
            self.lists.len(),
            self.readers.len(),
            self.memberships.len(),
            self.deliveries.len(),
            self.attempts.len(),
            self.unsubscribe_tokens.len(),
        ]
        .into_iter()
        .try_fold(0usize, usize::checked_add)
        .ok_or(MaviError::validation("mail_relocation_count_overflow"))?;
        i64::try_from(count).map_err(|_| MaviError::validation("mail_relocation_count_overflow"))
    }
}

impl MailService {
    #[allow(clippy::too_many_lines)]
    pub async fn export_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
    ) -> Result<MailRelocation> {
        let templates = sqlx::query(
            "select id, template_key, language, subject, body, content_type,
                    created_at, updated_at, deleted_at
               from mail_templates where site_id = $1 order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(MailTemplateRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                key: row
                    .try_get("template_key")
                    .map_err(|_| MaviError::Internal)?,
                language: row.try_get("language").map_err(|_| MaviError::Internal)?,
                subject: row.try_get("subject").map_err(|_| MaviError::Internal)?,
                body: row.try_get("body").map_err(|_| MaviError::Internal)?,
                content_type: row
                    .try_get("content_type")
                    .map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let lists = sqlx::query(
            "select id, slug, name, created_at, updated_at, deleted_at
               from mail_lists where site_id = $1 order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(MailListRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let readers = sqlx::query(
            "select id, email, name, standing, unsubscribe_token_hash,
                    created_at, updated_at, deleted_at
               from mail_readers where site_id = $1 order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(MailReaderRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                email: row.try_get("email").map_err(|_| MaviError::Internal)?,
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                standing: row.try_get("standing").map_err(|_| MaviError::Internal)?,
                unsubscribe_token_hash: row
                    .try_get("unsubscribe_token_hash")
                    .map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let memberships = sqlx::query(
            "select list_id, reader_id, created_at
               from mail_list_members where site_id = $1
              order by list_id asc, created_at asc, reader_id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(MailListMemberRelocation {
                list_id: row.try_get("list_id").map_err(|_| MaviError::Internal)?,
                reader_id: row.try_get("reader_id").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let deliveries = sqlx::query(
            "select id, template_id, list_id, recipient, subject, body, body_protected,
                    content_type, purpose, status, attempts, available_at, provider,
                    provider_reference, last_error, idempotency_key, created_at, updated_at,
                    sent_at
               from mail_deliveries where site_id = $1 order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            let status: String = row.try_get("status").map_err(|_| MaviError::Internal)?;
            let purpose: String = row.try_get("purpose").map_err(|_| MaviError::Internal)?;
            let body_protected: bool = row
                .try_get("body_protected")
                .map_err(|_| MaviError::Internal)?;
            Ok(MailDeliveryRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                template_id: row
                    .try_get("template_id")
                    .map_err(|_| MaviError::Internal)?,
                list_id: row.try_get("list_id").map_err(|_| MaviError::Internal)?,
                recipient: row.try_get("recipient").map_err(|_| MaviError::Internal)?,
                subject: row.try_get("subject").map_err(|_| MaviError::Internal)?,
                body: if body_protected {
                    PROTECTED_BODY_REDACTION.to_owned()
                } else {
                    row.try_get("body").map_err(|_| MaviError::Internal)?
                },
                body_protected,
                content_type: row
                    .try_get("content_type")
                    .map_err(|_| MaviError::Internal)?,
                purpose: purpose.clone(),
                status: normalize_protected_delivery_status(&status, body_protected, &purpose)
                    .to_owned(),
                attempts: row.try_get("attempts").map_err(|_| MaviError::Internal)?,
                available_at: row
                    .try_get("available_at")
                    .map_err(|_| MaviError::Internal)?,
                provider: row.try_get("provider").map_err(|_| MaviError::Internal)?,
                provider_reference: row
                    .try_get("provider_reference")
                    .map_err(|_| MaviError::Internal)?,
                last_error: row.try_get("last_error").map_err(|_| MaviError::Internal)?,
                idempotency_key: row
                    .try_get("idempotency_key")
                    .map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                sent_at: row.try_get("sent_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let attempts = sqlx::query(
            "select id, delivery_id, attempt_number, status, provider, provider_reference,
                    error, started_at, finished_at
               from mail_delivery_attempts where site_id = $1
              order by delivery_id asc, attempt_number asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            let status: String = row.try_get("status").map_err(|_| MaviError::Internal)?;
            let started_at: DateTime<Utc> =
                row.try_get("started_at").map_err(|_| MaviError::Internal)?;
            let finished_at: Option<DateTime<Utc>> = row
                .try_get("finished_at")
                .map_err(|_| MaviError::Internal)?;
            Ok(MailDeliveryAttemptRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                delivery_id: row
                    .try_get("delivery_id")
                    .map_err(|_| MaviError::Internal)?,
                attempt_number: row
                    .try_get("attempt_number")
                    .map_err(|_| MaviError::Internal)?,
                status: normalize_attempt_status(&status).to_owned(),
                provider: row.try_get("provider").map_err(|_| MaviError::Internal)?,
                provider_reference: row
                    .try_get("provider_reference")
                    .map_err(|_| MaviError::Internal)?,
                error: row.try_get("error").map_err(|_| MaviError::Internal)?,
                started_at,
                finished_at: if status == "sending" {
                    Some(finished_at.unwrap_or(started_at))
                } else {
                    finished_at
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let unsubscribe_tokens = sqlx::query(
            "select id, delivery_id, reader_id, token_hash, created_at, used_at
               from mail_unsubscribe_tokens where site_id = $1
              order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(MailUnsubscribeTokenRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                delivery_id: row
                    .try_get("delivery_id")
                    .map_err(|_| MaviError::Internal)?,
                reader_id: row.try_get("reader_id").map_err(|_| MaviError::Internal)?,
                token_hash: row.try_get("token_hash").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                used_at: row.try_get("used_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let relocation = MailRelocation {
            format: MAIL_RELOCATION_FORMAT.to_owned(),
            version: MAIL_RELOCATION_VERSION,
            source_site_id: context.site_id,
            templates,
            lists,
            readers,
            memberships,
            deliveries,
            attempts,
            unsubscribe_tokens,
        };
        relocation.validate_for_relocation(context.site_id)?;
        Ok(relocation)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn import_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        relocation: &MailRelocation,
    ) -> Result<()> {
        relocation.validate_for_relocation(context.site_id)?;

        for template in &relocation.templates {
            sqlx::query(
                "insert into mail_templates
                    (site_id, id, template_key, language, subject, body, content_type,
                     created_at, updated_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 on conflict (site_id, id) do update set
                    template_key = excluded.template_key, language = excluded.language,
                    subject = excluded.subject, body = excluded.body,
                    content_type = excluded.content_type, created_at = excluded.created_at,
                    updated_at = excluded.updated_at, deleted_at = excluded.deleted_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(template.id)
            .bind(&template.key)
            .bind(&template.language)
            .bind(&template.subject)
            .bind(&template.body)
            .bind(&template.content_type)
            .bind(template.created_at)
            .bind(template.updated_at)
            .bind(template.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        for list in &relocation.lists {
            sqlx::query(
                "insert into mail_lists
                    (site_id, id, slug, name, created_at, updated_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7)
                 on conflict (site_id, id) do update set
                    slug = excluded.slug, name = excluded.name,
                    created_at = excluded.created_at, updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(list.id)
            .bind(&list.slug)
            .bind(&list.name)
            .bind(list.created_at)
            .bind(list.updated_at)
            .bind(list.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        for reader in &relocation.readers {
            sqlx::query(
                "insert into mail_readers
                    (site_id, id, email, name, standing, unsubscribe_token_hash,
                     created_at, updated_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 on conflict (site_id, id) do update set
                    email = excluded.email, name = excluded.name, standing = excluded.standing,
                    unsubscribe_token_hash = excluded.unsubscribe_token_hash,
                    created_at = excluded.created_at, updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(reader.id)
            .bind(&reader.email)
            .bind(&reader.name)
            .bind(&reader.standing)
            .bind(&reader.unsubscribe_token_hash)
            .bind(reader.created_at)
            .bind(reader.updated_at)
            .bind(reader.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        for membership in &relocation.memberships {
            sqlx::query(
                "insert into mail_list_members (site_id, list_id, reader_id, created_at)
                 values ($1, $2, $3, $4)
                 on conflict (site_id, list_id, reader_id) do update set
                    created_at = excluded.created_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(membership.list_id)
            .bind(membership.reader_id)
            .bind(membership.created_at)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        for delivery in &relocation.deliveries {
            // Relocation snapshots intentionally never contain ciphertext.
            // Remove any target-side secret for the same delivery id before
            // upserting the redacted/plain snapshot so an old target row
            // cannot make a stale token sendable after cutover.
            sqlx::query(
                "delete from mail_delivery_secrets
                  where site_id = $1 and delivery_id = $2",
            )
            .bind(context.site_id.into_uuid())
            .bind(delivery.id)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            sqlx::query(
                "delete from mail_delivery_links
                  where site_id = $1 and delivery_id = $2",
            )
            .bind(context.site_id.into_uuid())
            .bind(delivery.id)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            sqlx::query(
                "delete from mail_unsubscribe_tokens
                  where site_id = $1 and delivery_id = $2",
            )
            .bind(context.site_id.into_uuid())
            .bind(delivery.id)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            sqlx::query(
                "insert into mail_deliveries
                    (site_id, id, template_id, list_id, recipient, subject, body, body_protected,
                     content_type, purpose, status, attempts, available_at, lease_owner, lease_until,
                     provider, provider_reference, last_error, idempotency_key,
                     created_at, updated_at, sent_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, null, null,
                         $14, $15, $16, $17, $18, $19, $20)
                 on conflict (site_id, id) do update set
                    template_id = excluded.template_id, list_id = excluded.list_id,
                    recipient = excluded.recipient, subject = excluded.subject,
                    body = excluded.body, content_type = excluded.content_type,
                    body_protected = excluded.body_protected, purpose = excluded.purpose,
                    status = excluded.status,
                    attempts = excluded.attempts, available_at = excluded.available_at,
                    lease_owner = null, lease_until = null, provider = excluded.provider,
                    provider_reference = excluded.provider_reference,
                    last_error = excluded.last_error, idempotency_key = excluded.idempotency_key,
                    created_at = excluded.created_at, updated_at = excluded.updated_at,
                    sent_at = excluded.sent_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(delivery.id)
            .bind(delivery.template_id)
            .bind(delivery.list_id)
            .bind(&delivery.recipient)
            .bind(&delivery.subject)
            .bind(&delivery.body)
            .bind(delivery.body_protected)
            .bind(&delivery.content_type)
            .bind(&delivery.purpose)
            .bind(normalize_protected_delivery_status(
                &delivery.status,
                delivery.body_protected,
                &delivery.purpose,
            ))
            .bind(delivery.attempts)
            .bind(delivery.available_at)
            .bind(&delivery.provider)
            .bind(&delivery.provider_reference)
            .bind(&delivery.last_error)
            .bind(&delivery.idempotency_key)
            .bind(delivery.created_at)
            .bind(delivery.updated_at)
            .bind(delivery.sent_at)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        for attempt in &relocation.attempts {
            sqlx::query(
                "insert into mail_delivery_attempts
                    (site_id, id, delivery_id, attempt_number, status, provider,
                     provider_reference, error, started_at, finished_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 on conflict (site_id, id) do update set
                    delivery_id = excluded.delivery_id, attempt_number = excluded.attempt_number,
                    status = excluded.status, provider = excluded.provider,
                    provider_reference = excluded.provider_reference, error = excluded.error,
                    started_at = excluded.started_at, finished_at = excluded.finished_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(attempt.id)
            .bind(attempt.delivery_id)
            .bind(attempt.attempt_number)
            .bind(&attempt.status)
            .bind(&attempt.provider)
            .bind(&attempt.provider_reference)
            .bind(&attempt.error)
            .bind(attempt.started_at)
            .bind(attempt.finished_at)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        for token in &relocation.unsubscribe_tokens {
            sqlx::query(
                "insert into mail_unsubscribe_tokens
                    (site_id, id, delivery_id, reader_id, token_hash, created_at, used_at)
                 values ($1, $2, $3, $4, $5, $6, $7)
                 on conflict (site_id, id) do update set
                    delivery_id = excluded.delivery_id, reader_id = excluded.reader_id,
                    token_hash = excluded.token_hash, created_at = excluded.created_at,
                    used_at = excluded.used_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(token.id)
            .bind(token.delivery_id)
            .bind(token.reader_id)
            .bind(&token.token_hash)
            .bind(token.created_at)
            .bind(token.used_at)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "portable.mail.relocated".to_owned(),
                    resource_type: "MailRelocation".to_owned(),
                    resource_id: None,
                    payload: json!({
                        "templates": relocation.templates.len(),
                        "lists": relocation.lists.len(),
                        "readers": relocation.readers.len(),
                        "memberships": relocation.memberships.len(),
                        "deliveries": relocation.deliveries.len(),
                        "attempts": relocation.attempts.len(),
                        "unsubscribe_tokens": relocation.unsubscribe_tokens.len(),
                        "leases_reset": true,
                        "provider_credentials": "separate_capability",
                    }),
                },
            )
            .await
    }
}

fn normalize_delivery_status(status: &str) -> &str {
    if status == "sending" { "retry" } else { status }
}

fn normalize_protected_delivery_status<'a>(
    status: &'a str,
    body_protected: bool,
    purpose: &str,
) -> &'a str {
    if (body_protected || purpose == "campaign") && matches!(status, "queued" | "retry" | "sending")
    {
        "cancelled"
    } else {
        normalize_delivery_status(status)
    }
}

fn normalize_attempt_status(status: &str) -> &str {
    if status == "sending" { "retry" } else { status }
}

fn valid_template_key(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 64
        && value.chars().enumerate().all(|(index, character)| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || (index > 0 && character == '_')
        })
}

fn valid_language(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 35
        && value.split('-').all(|part| {
            (2..=8).contains(&part.chars().count())
                && part
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
}

fn valid_subject(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= 300 && !value.chars().any(char::is_control)
}

fn valid_body(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= 100_000 && !value.contains('\0')
}

fn valid_content_type(value: &str) -> bool {
    matches!(value, "plain" | "html")
}

fn valid_list_slug(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 64
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn valid_name(value: &str, max_chars: usize) -> bool {
    !value.is_empty() && value.chars().count() <= max_chars && !value.chars().any(char::is_control)
}

fn valid_email(value: &str) -> bool {
    Email::parse(value).is_ok_and(|email| email.as_str() == value)
}

fn valid_optional_text(value: Option<&str>, max_chars: usize) -> bool {
    value.is_none_or(|value| {
        value.chars().count() <= max_chars && !value.chars().any(char::is_control)
    })
}

fn valid_optional_key(value: Option<&str>, max_chars: usize) -> bool {
    value.is_none_or(|value| {
        !value.is_empty()
            && value.chars().count() <= max_chars
            && !value.chars().any(char::is_control)
    })
}

mod base64_bytes {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&URL_SAFE_NO_PAD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(serde::de::Error::custom)
    }
}

fn map_write_error(error: &sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = &error
        && matches!(
            database.constraint(),
            Some(
                "mail_deliveries_site_idempotency"
                    | "mail_templates_site_key_language_active"
                    | "mail_lists_site_slug_active"
                    | "mail_readers_site_email_active"
                    | "mail_readers_site_unsubscribe_token"
                    | "mail_unsubscribe_tokens_site_token_hash"
                    | "mail_unsubscribe_tokens_site_delivery",
            )
        )
    {
        return MaviError::conflict("mail_relocation_conflict");
    }
    MaviError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sending_leases_are_normalized_to_retry() {
        assert_eq!(normalize_delivery_status("sending"), "retry");
        assert_eq!(normalize_attempt_status("sending"), "retry");
        assert_eq!(normalize_delivery_status("sent"), "sent");
        assert_eq!(
            normalize_protected_delivery_status("queued", true, "transactional"),
            "cancelled"
        );
        assert_eq!(
            normalize_protected_delivery_status("sent", true, "transactional"),
            "sent"
        );
        assert_eq!(
            normalize_protected_delivery_status("queued", false, "campaign"),
            "cancelled"
        );
    }

    #[test]
    fn relocation_requires_site_and_provider_neutral_state() {
        let site = SiteId::new();
        let relocation = MailRelocation::empty(site);
        relocation
            .validate_for_relocation(site)
            .expect("empty relocation");
        assert!(relocation.validate_for_relocation(SiteId::new()).is_err());
        assert_eq!(relocation.record_count().expect("count"), 0);
    }
}
