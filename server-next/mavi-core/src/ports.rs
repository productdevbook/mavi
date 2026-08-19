use std::{fmt::Debug, future::Future, pin::Pin};

use crate::{MailDeliveryId, MailSender, Money, Result, SiteContext};
use serde::{Deserialize, Serialize};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait Clock: Debug + Send + Sync {
    fn now(&self) -> chrono::DateTime<chrono::Utc>;
}

pub trait FileStore: Debug + Send + Sync {
    /// Stores bytes under a site-scoped key, replacing an existing object.
    /// Implementations must make retries safe for the same `(site, path)`.
    fn put<'a>(
        &'a self,
        context: &'a SiteContext,
        path: &'a str,
        bytes: Vec<u8>,
    ) -> BoxFuture<'a, Result<()>>;
    fn get<'a>(&'a self, context: &'a SiteContext, path: &'a str)
    -> BoxFuture<'a, Result<Vec<u8>>>;
    fn remove<'a>(&'a self, context: &'a SiteContext, path: &'a str) -> BoxFuture<'a, Result<()>>;
    /// Lists site-scoped object keys in deterministic order.
    ///
    /// Adapters must not cross the site namespace and must ignore entries
    /// that cannot be represented as safe storage keys. The worker applies a
    /// second domain-level allowlist before deleting anything returned here.
    fn list<'a>(&'a self, context: &'a SiteContext) -> BoxFuture<'a, Result<Vec<String>>>;
}

pub trait Mailer: Debug + Send + Sync {
    fn send<'a>(
        &'a self,
        context: &'a SiteContext,
        request: MailDeliveryRequest,
    ) -> BoxFuture<'a, Result<MailDeliveryReceipt>>;
}

/// Whether a provider-facing message is a one-to-one system message or a
/// list delivery. Providers use this to apply the correct deliverability
/// policy without inspecting template text.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MailDeliveryPurpose {
    Transactional,
    Campaign,
}

impl MailDeliveryPurpose {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transactional => "transactional",
            Self::Campaign => "campaign",
        }
    }
}

/// The complete provider-facing contract for one outbox attempt.
///
/// Providers must use `delivery_id` and the optional idempotency key when the
/// remote API supports deduplication. `attempt_number` is the durable attempt
/// opened by the database before the provider call; it is not a local retry
/// counter that an adapter may change. Keeping these fields outside
/// [`MailMessage`] prevents transport metadata from leaking into template and
/// rendering code.
#[derive(Clone, Debug)]
pub struct MailDeliveryRequest {
    pub delivery_id: MailDeliveryId,
    pub attempt_number: u16,
    pub idempotency_key: Option<String>,
    pub purpose: MailDeliveryPurpose,
    /// The site-level sender captured when the delivery was queued.
    /// `None` lets the deployment policy supply its backwards-compatible
    /// default for sites that have not configured a sender yet.
    pub sender: Option<MailSender>,
    pub message: MailMessage,
}

#[derive(Clone, Debug)]
pub struct MailMessage {
    pub recipient: String,
    pub subject: String,
    pub body: String,
    pub content_type: MailContentType,
    /// A bearer URL for list deliveries. Provider adapters should emit it as
    /// `List-Unsubscribe` and `List-Unsubscribe-Post`; it is never exposed by
    /// the administrative delivery DTO.
    pub unsubscribe_url: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MailContentType {
    #[default]
    Plain,
    Html,
}

impl MailContentType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Html => "html",
        }
    }
}

#[derive(Clone, Debug)]
pub struct MailDeliveryReceipt {
    pub provider: String,
    pub reference: String,
}

pub trait Payments: Debug + Send + Sync {
    fn charge<'a>(
        &'a self,
        context: &'a SiteContext,
        amount: Money,
    ) -> BoxFuture<'a, Result<PaymentReceipt>>;
}

#[derive(Clone, Debug)]
pub struct PaymentReceipt {
    pub provider: String,
    pub reference: String,
}

pub trait Builds: Debug + Send + Sync {
    fn build<'a>(
        &'a self,
        context: &'a SiteContext,
        source: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<u8>>>;
}

pub trait Seals: Debug + Send + Sync {
    fn seal<'a>(
        &'a self,
        context: &'a SiteContext,
        value: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<u8>>>;
    fn unseal<'a>(
        &'a self,
        context: &'a SiteContext,
        value: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<u8>>>;
}
