//! Site-scoped mail configuration and delivery orchestration.
//!
//! Templates, subscriber lists and outbox rows are application data. A web
//! request may enqueue a delivery, but it never calls a provider. Workers
//! claim rows with a lease, call the [`mavi_core::ports::Mailer`] adapter, and
//! record the provider result in a separate transaction.

mod deliveries;
mod lists;
mod relocation;
mod templates;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_core::{Cursor, MaviError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use deliveries::{
    ClaimedDelivery, DeliveryListFilter, EnqueueDelivery, MAX_DELIVERY_ATTEMPTS, MailDelivery,
    MailDeliveryStatus, MailPurpose, MailServiceError, PROTECTED_BODY_REDACTION, RetryDelivery,
    SendCampaign, SendCount,
};
pub use lists::{
    AddReader, CreateMailList, MailList, MailListListFilter, MailReader, MailReaderCreated,
    MailStanding, ReaderListFilter, UnsubscribeReceipt, UpdateMailList,
};
pub use mavi_core::ports::MailDeliveryRequest;
pub use relocation::{
    MAIL_RELOCATION_FORMAT, MAIL_RELOCATION_VERSION, MAX_MAIL_RELOCATION_BYTES,
    MAX_MAIL_RELOCATION_RECORDS, MailDeliveryAttemptRelocation, MailDeliveryRelocation,
    MailListMemberRelocation, MailListRelocation, MailReaderRelocation, MailRelocation,
    MailTemplateRelocation,
};
pub use templates::{
    CreateMailTemplate, MailContentType, MailTemplate, MailTemplateListFilter, MailTemplatePreview,
    RenderedMail, UpdateMailTemplate,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct MailService;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RecentCursor {
    pub created_at: DateTime<Utc>,
    pub id: Uuid,
}

pub(crate) fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes =
        serde_json::to_vec(&RecentCursor { created_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

pub(crate) fn decode_cursor(cursor: &Cursor) -> Result<RecentCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

#[must_use]
pub fn api() -> mavi_contract::Api {
    let mut api = mavi_contract::Api::default();
    api.extend(templates::api());
    api.extend(lists::api());
    api.extend(deliveries::api());
    api
}

/// Calls a provider adapter for a worker-claimed message.
///
/// This function intentionally has no database handle. Claiming and marking
/// state happen outside the provider call, so a slow SMTP/API provider cannot
/// keep a `PostgreSQL` transaction open.
pub async fn send_via<M: mavi_core::ports::Mailer + ?Sized>(
    context: &mavi_core::SiteContext,
    mailer: &M,
    request: MailDeliveryRequest,
) -> Result<mavi_core::ports::MailDeliveryReceipt> {
    mailer.send(context, request).await
}
