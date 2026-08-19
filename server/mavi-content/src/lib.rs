//! Content domain and its application-facing commands.
//!
//! The HTTP layer does not construct SQL and does not decide publication
//! semantics. It hands a validated command to this crate; the repository is
//! the only place that knows the content tables.

mod content_types;
mod revisions;

use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Api, Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, ContentId, Cursor, MaviError, Page, PageRequest, Result, SiteContext,
    SiteId, TermId,
};
use mavi_jobs::{JobKind, JobsService};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub use content_types::{
    CONTENT_FIELD_REQUIRED, ContentType, ContentTypeField, ContentTypeListFilter,
    DeclareContentType, FieldKind,
};
pub use revisions::{ContentRevision, ContentRevisionListFilter};

const KIND_MAX: usize = 31;
const LANGUAGE_MAX: usize = 35;
const SLUG_MAX: usize = 160;
const TITLE_MAX: usize = 200;

pub const CONTENT_NOT_FOUND: &str = "content_not_found";
pub const CONTENT_SLUG_TAKEN: &str = "content_slug_taken";
pub const CONTENT_KIND_INVALID: &str = "content_kind_invalid";
pub const CONTENT_LANGUAGE_INVALID: &str = "content_language_invalid";
pub const CONTENT_SLUG_INVALID: &str = "content_slug_invalid";
pub const CONTENT_TITLE_INVALID: &str = "content_title_invalid";
pub const CONTENT_FIELDS_INVALID: &str = "content_fields_invalid";
pub const CONTENT_STATE_INVALID: &str = "content_state_invalid";

/// Durable work that publishes an entry when its scheduled publication time
/// is reached. The worker must re-check the entry's current scheduled state so
/// a superseded schedule becomes a safe no-op.
pub const SCHEDULED_PUBLISH_JOB: JobKind = JobKind::new("content.publish_scheduled", 5);

/// Canonical content routes. Generated clients and documentation consume this
/// declaration; handlers are not allowed to invent a parallel route shape.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn api() -> Api {
    let mut api = Api::new([
        Endpoint::new(
            Method::Get,
            "/api/v1/content",
            "content.list",
            "List site content",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Content,
            action: Action::View,
        })
        .takes_query("ContentListFilter")
        .returns(200, "ContentPage"),
        Endpoint::new(
            Method::Get,
            "/api/v1/content/{id}",
            "content.read",
            "Read site content",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Content,
            action: Action::View,
        })
        .returns(200, "Content"),
        Endpoint::new(
            Method::Post,
            "/api/v1/content",
            "content.create",
            "Create site content",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Content,
            action: Action::Write,
        })
        .takes("CreateContent")
        .returns(201, "Content")
        .changes(false),
        Endpoint::new(
            Method::Patch,
            "/api/v1/content/{id}",
            "content.update",
            "Update site content",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Content,
            action: Action::Write,
        })
        .takes("UpdateContent")
        .returns(200, "Content")
        .changes(true),
        Endpoint::new(
            Method::Post,
            "/api/v1/content/{id}/publish",
            "content.publish",
            "Publish site content",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Publish,
            action: Action::Write,
        })
        .returns(200, "Content")
        .changes(false),
        Endpoint::new(
            Method::Post,
            "/api/v1/content/{id}/schedule",
            "content.schedule",
            "Schedule site content",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Publish,
            action: Action::Write,
        })
        .takes("ScheduleContent")
        .returns(200, "Content")
        .changes(false),
        Endpoint::new(
            Method::Post,
            "/api/v1/content/{id}/archive",
            "content.archive",
            "Archive site content",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Publish,
            action: Action::Write,
        })
        .returns(200, "Content")
        .changes(false),
        Endpoint::new(
            Method::Delete,
            "/api/v1/content/{id}",
            "content.trash",
            "Move site content to trash",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Trash,
            action: Action::Delete,
        })
        .returns(204, "Empty")
        .changes(false),
        Endpoint::new(
            Method::Post,
            "/api/v1/content/{id}/restore",
            "content.restore",
            "Restore site content from trash",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Trash,
            action: Action::Write,
        })
        .returns(200, "Content")
        .changes(false),
        Endpoint::new(
            Method::Get,
            "/public/v1/content/{slug}",
            "content.public_read",
            "Read published content",
        )
        .public()
        .takes_query("PublicContentQuery")
        .returns(200, "Content"),
    ]);
    api.endpoints.extend(content_types::endpoints());
    api.endpoints.extend(revisions::endpoints());
    let mut shapes = content_shapes();
    shapes.extend(content_types::shapes());
    shapes.extend(revisions::shapes());
    api.with_shapes(shapes)
}

#[allow(clippy::too_many_lines)]
fn content_shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "PublicationStatus",
            json!({"type": "string", "enum": ["draft", "scheduled", "published", "archived"]}),
        ),
        Shape::new(
            "ContentListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                    "kind": {"type": ["string", "null"], "maxLength": 31},
                    "language": {"type": ["string", "null"], "maxLength": 35},
                    "status": {"$ref": "#/components/schemas/PublicationStatus"},
                },
            }),
        ),
        Shape::new(
            "PublicContentQuery",
            json!({
                "type": "object",
                "properties": {
                    "language": {"type": ["string", "null"], "maxLength": 35},
                },
            }),
        ),
        Shape::new(
            "Publication",
            json!({
                "oneOf": [
                    {"type": "string", "enum": ["draft", "archived"]},
                    {"type": "object", "required": ["scheduled"], "properties": {"scheduled": {"type": "object", "required": ["at"], "properties": {"at": {"type": "string", "format": "date-time"}}}}},
                    {"type": "object", "required": ["published"], "properties": {"published": {"type": "object", "required": ["at"], "properties": {"at": {"type": "string", "format": "date-time"}}}}},
                ],
            }),
        ),
        Shape::new(
            "PublicationInput",
            json!({
                "oneOf": [
                    {"type": "string", "enum": ["draft", "publish", "archive"]},
                    {"type": "object", "required": ["schedule"], "properties": {"schedule": {"type": "string", "format": "date-time"}}},
                ],
            }),
        ),
        Shape::new(
            "CreateContent",
            json!({
                "type": "object",
                "required": ["kind", "language", "slug", "title"],
                "properties": {
                    "kind": {"type": "string", "maxLength": 31},
                    "language": {"type": "string", "maxLength": 35},
                    "slug": {"type": "string", "maxLength": 160},
                    "title": {"type": "string", "maxLength": 200},
                    "excerpt": {"type": ["string", "null"]},
                    "body": {"type": "string"},
                    "fields": {"type": "object", "additionalProperties": true},
                    "publication": {"$ref": "#/components/schemas/PublicationInput"},
                },
            }),
        ),
        Shape::new(
            "UpdateContent",
            json!({
                "type": "object",
                "properties": {
                    "slug": {"type": ["string", "null"], "maxLength": 160},
                    "title": {"type": ["string", "null"], "maxLength": 200},
                    "excerpt": {"type": ["string", "null"]},
                    "body": {"type": ["string", "null"]},
                    "fields": {"type": ["object", "null"], "additionalProperties": true},
                    "publication": {"$ref": "#/components/schemas/PublicationInput"},
                },
            }),
        ),
        Shape::new(
            "ScheduleContent",
            json!({
                "type": "object",
                "required": ["at"],
                "properties": {"at": {"type": "string", "format": "date-time"}},
            }),
        ),
        Shape::new(
            "Content",
            json!({
                "type": "object",
                "required": ["id", "site_id", "kind", "language", "slug", "title", "excerpt", "body", "fields", "publication", "revision", "created_at", "updated_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "site_id": {"type": "string", "format": "uuid"},
                    "kind": {"type": "string"},
                    "language": {"type": "string"},
                    "slug": {"type": "string"},
                    "title": {"type": "string"},
                    "excerpt": {"type": ["string", "null"]},
                    "body": {"type": "string"},
                    "fields": {"type": "object", "additionalProperties": true},
                    "publication": {"$ref": "#/components/schemas/Publication"},
                    "revision": {"type": "integer", "minimum": 1},
                    "created_at": {"type": "string", "format": "date-time"},
                    "updated_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "ContentPage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/Content"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512},
                },
            }),
        ),
    ]
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ContentKind(String);

impl ContentKind {
    pub fn parse(value: &str) -> Result<Self> {
        if value.is_empty()
            || value.len() > KIND_MAX
            || !value.starts_with(|character: char| character.is_ascii_lowercase())
            || !value.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
            })
        {
            return Err(MaviError::validation(CONTENT_KIND_INVALID));
        }

        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct LanguageTag(String);

impl LanguageTag {
    pub fn parse(value: &str) -> Result<Self> {
        let valid = !value.is_empty()
            && value.len() <= LANGUAGE_MAX
            && value.split('-').enumerate().all(|(index, part)| {
                let length = part.len();
                let size_ok = if index == 0 {
                    (2..=8).contains(&length)
                } else {
                    (1..=8).contains(&length)
                };
                size_ok
                    && part.chars().all(|character| {
                        if index == 0 {
                            character.is_ascii_alphabetic()
                        } else {
                            character.is_ascii_alphanumeric()
                        }
                    })
            });

        if !valid {
            return Err(MaviError::validation(CONTENT_LANGUAGE_INVALID));
        }

        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Slug(String);

impl Slug {
    pub fn parse(value: &str) -> Result<Self> {
        let valid = !value.is_empty()
            && value.len() <= SLUG_MAX
            && value.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
            && !value.starts_with('-')
            && !value.ends_with('-')
            && !value.contains("--");

        if !valid {
            return Err(MaviError::validation(CONTENT_SLUG_INVALID));
        }

        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Title(String);

impl Title {
    pub fn parse(value: &str) -> Result<Self> {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.chars().count() > TITLE_MAX {
            return Err(MaviError::validation(CONTENT_TITLE_INVALID));
        }

        Ok(Self(trimmed.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Publication {
    Draft,
    Scheduled { at: DateTime<Utc> },
    Published { at: DateTime<Utc> },
    Archived,
}

impl Publication {
    fn columns(&self) -> (&'static str, Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
        match self {
            Self::Draft => ("draft", None, None),
            Self::Scheduled { at } => ("scheduled", Some(*at), None),
            Self::Published { at } => ("published", None, Some(*at)),
            Self::Archived => ("archived", None, None),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct CreateContent {
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub excerpt: Option<String>,
    #[serde(default)]
    pub body: String,
    #[serde(default = "empty_fields")]
    pub fields: Value,
    #[serde(default)]
    pub publication: PublicationInput,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PublicationInput {
    #[default]
    Draft,
    Schedule(DateTime<Utc>),
    Publish,
    Archive,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationStatus {
    Draft,
    Scheduled,
    Published,
    Archived,
}

impl PublicationStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Scheduled => "scheduled",
            Self::Published => "published",
            Self::Archived => "archived",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentListFilter {
    pub kind: Option<String>,
    pub language: Option<String>,
    pub status: Option<PublicationStatus>,
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScheduleContent {
    pub at: DateTime<Utc>,
}

/// Payload for [`SCHEDULED_PUBLISH_JOB`]. The timestamp is part of the
/// payload and idempotency key so a rescheduled entry cannot be published by
/// an older job.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduledPublishJob {
    pub content_id: ContentId,
    pub scheduled_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduledPublishSkipReason {
    Missing,
    Deleted,
    NoLongerScheduled,
    DifferentSchedule,
}

impl ScheduledPublishSkipReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Deleted => "deleted",
            Self::NoLongerScheduled => "no_longer_scheduled",
            Self::DifferentSchedule => "different_schedule",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScheduledPublishOutcome {
    Published(Box<Content>),
    Skipped(ScheduledPublishSkipReason),
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct UpdateContent {
    pub slug: Option<String>,
    pub title: Option<String>,
    pub excerpt: Option<Option<String>>,
    pub body: Option<String>,
    pub fields: Option<Value>,
    pub publication: Option<PublicationInput>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Content {
    pub id: ContentId,
    pub site_id: SiteId,
    pub kind: ContentKind,
    pub language: LanguageTag,
    pub slug: Slug,
    pub title: String,
    pub excerpt: Option<String>,
    pub body: String,
    pub fields: Value,
    pub publication: Publication,
    pub revision: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ScheduledPublishJob {
    #[must_use]
    pub fn from_content(content: &Content) -> Option<Self> {
        match &content.publication {
            Publication::Scheduled { at } => Some(Self {
                content_id: content.id,
                scheduled_at: *at,
            }),
            Publication::Draft | Publication::Published { .. } | Publication::Archived => None,
        }
    }

    #[must_use]
    pub fn idempotency_key(&self) -> String {
        format!(
            "content.schedule:{}:{}",
            self.content_id,
            self.scheduled_at
                .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
        )
    }
}

fn empty_fields() -> Value {
    Value::Object(serde_json::Map::new())
}

fn validate_fields(fields: &Value) -> Result<()> {
    if fields.is_object() {
        Ok(())
    } else {
        Err(MaviError::validation(CONTENT_FIELDS_INVALID))
    }
}

fn publication_input(input: &PublicationInput, now: DateTime<Utc>) -> Result<Publication> {
    match input {
        PublicationInput::Draft => Ok(Publication::Draft),
        PublicationInput::Schedule(at) => {
            let at = normalize_database_timestamp(*at)?;
            if at > now {
                Ok(Publication::Scheduled { at })
            } else {
                Err(MaviError::validation(CONTENT_STATE_INVALID))
            }
        }
        PublicationInput::Publish => Ok(Publication::Published { at: now }),
        PublicationInput::Archive => Ok(Publication::Archived),
    }
}

fn normalize_database_timestamp(value: DateTime<Utc>) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp_micros(value.timestamp_micros()).ok_or(MaviError::Internal)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ContentCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes =
        serde_json::to_vec(&ContentCursor { created_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor(cursor: &Cursor) -> Result<ContentCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

fn checked_new(input: &CreateContent, now: DateTime<Utc>) -> Result<NewContent> {
    let kind = ContentKind::parse(&input.kind)?;
    let language = LanguageTag::parse(&input.language)?;
    let slug = Slug::parse(&input.slug)?;
    let title = Title::parse(&input.title)?;
    validate_fields(&input.fields)?;

    if input.body.chars().count() > 1_000_000 {
        return Err(MaviError::validation("content_body_too_large"));
    }

    Ok(NewContent {
        kind,
        language,
        slug,
        title,
        excerpt: input.excerpt.clone(),
        body: input.body.clone(),
        fields: input.fields.clone(),
        publication: publication_input(&input.publication, now)?,
    })
}

struct NewContent {
    kind: ContentKind,
    language: LanguageTag,
    slug: Slug,
    title: Title,
    excerpt: Option<String>,
    body: String,
    fields: Value,
    publication: Publication,
}

fn status(
    value: &str,
    scheduled_at: Option<DateTime<Utc>>,
    published_at: Option<DateTime<Utc>>,
) -> Result<Publication> {
    match value {
        "draft" if scheduled_at.is_none() && published_at.is_none() => Ok(Publication::Draft),
        "scheduled" if scheduled_at.is_some() && published_at.is_none() => {
            Ok(Publication::Scheduled {
                at: scheduled_at.expect("checked above"),
            })
        }
        "published" if scheduled_at.is_none() && published_at.is_some() => {
            Ok(Publication::Published {
                at: published_at.expect("checked above"),
            })
        }
        "archived" if scheduled_at.is_none() && published_at.is_none() => Ok(Publication::Archived),
        _ => Err(MaviError::Internal),
    }
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<Content> {
    let status_value: String = row.try_get("status").map_err(|_| MaviError::Internal)?;
    let scheduled_at = row
        .try_get("scheduled_at")
        .map_err(|_| MaviError::Internal)?;
    let published_at = row
        .try_get("published_at")
        .map_err(|_| MaviError::Internal)?;
    let revision: i32 = row.try_get("revision").map_err(|_| MaviError::Internal)?;

    Ok(Content {
        id: ContentId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        site_id: SiteId::from_uuid(row.try_get("site_id").map_err(|_| MaviError::Internal)?),
        kind: ContentKind::parse(
            &row.try_get::<String, _>("kind")
                .map_err(|_| MaviError::Internal)?,
        )?,
        language: LanguageTag::parse(
            &row.try_get::<String, _>("language")
                .map_err(|_| MaviError::Internal)?,
        )?,
        slug: Slug::parse(
            &row.try_get::<String, _>("slug")
                .map_err(|_| MaviError::Internal)?,
        )?,
        title: row.try_get("title").map_err(|_| MaviError::Internal)?,
        excerpt: row.try_get("excerpt").map_err(|_| MaviError::Internal)?,
        body: row.try_get("body").map_err(|_| MaviError::Internal)?,
        fields: row.try_get("fields").map_err(|_| MaviError::Internal)?,
        publication: status(&status_value, scheduled_at, published_at)?,
        revision: u32::try_from(revision).map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

/// Application service for content commands. The caller must provide a
/// transaction opened from the request's [`SiteContext`].
#[derive(Clone, Copy, Debug, Default)]
pub struct ContentService;

#[derive(Clone, Copy, Debug)]
struct UpdateAudit<'a> {
    action: &'a str,
    source_revision: Option<u32>,
}

impl ContentService {
    pub async fn initialize(&self, tx: &mut SiteTx, context: &SiteContext) -> Result<()> {
        content_types::initialize(tx, context).await
    }

    pub async fn enqueue_scheduled_publish(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        jobs: &JobsService,
        entry: &Content,
    ) -> Result<()> {
        let Some(job) = ScheduledPublishJob::from_content(entry) else {
            return Ok(());
        };
        let payload = serde_json::to_value(&job).map_err(|_| MaviError::Internal)?;
        let idempotency_key = job.idempotency_key();
        jobs.enqueue(
            tx,
            context,
            SCHEDULED_PUBLISH_JOB.name,
            &payload,
            Some(job.scheduled_at),
            Some(&idempotency_key),
        )
        .await
        .map(|_| ())
    }

    pub async fn list_content_types(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &ContentTypeListFilter,
    ) -> Result<Page<ContentType>> {
        content_types::list(tx, context, filter).await
    }

    pub async fn upsert_content_type(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        kind: &str,
        input: &DeclareContentType,
    ) -> Result<ContentType> {
        content_types::upsert(tx, context, kind, input).await
    }

    pub async fn delete_content_type(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        kind: &str,
    ) -> Result<()> {
        content_types::delete(tx, context, kind).await
    }

    pub async fn list_revisions(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ContentId,
        filter: &ContentRevisionListFilter,
    ) -> Result<Page<ContentRevision>> {
        revisions::list(tx, context, id, filter).await
    }

    pub async fn read_revision(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ContentId,
        revision: u32,
    ) -> Result<ContentRevision> {
        revisions::read(tx, context, id, revision).await
    }

    pub async fn create(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateContent,
        now: DateTime<Utc>,
    ) -> Result<Content> {
        let new = checked_new(input, now)?;
        content_types::validate_content_fields(tx, context, &new.kind, &new.fields).await?;
        let (status, scheduled_at, published_at) = new.publication.columns();

        let row = sqlx::query(
            "insert into content_entries (site_id, id, kind, language, slug, title, excerpt, body, fields, status, scheduled_at, published_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             returning id, site_id, kind, language, slug, title, excerpt, body, fields, status, scheduled_at, published_at, revision, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(Uuid::now_v7())
        .bind(new.kind.as_str())
        .bind(new.language.as_str())
        .bind(new.slug.as_str())
        .bind(new.title.as_str())
        .bind(new.excerpt)
        .bind(new.body)
        .bind(new.fields)
        .bind(status)
        .bind(scheduled_at)
        .bind(published_at)
        .fetch_one(tx.conn())
        .await
        .map_err(|error| map_write_error(&error))?;

        let created = from_row(&row)?;
        let (revision_status, revision_scheduled_at, revision_published_at) =
            created.publication.columns();
        sqlx::query(
            "insert into content_revisions (site_id, content_id, revision, kind, language, slug, title, excerpt, body, fields, status, scheduled_at, published_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(context.site_id.into_uuid())
        .bind(created.id.into_uuid())
        .bind(1_i32)
        .bind(created.kind.as_str())
        .bind(created.language.as_str())
        .bind(created.slug.as_str())
        .bind(&created.title)
        .bind(&created.excerpt)
        .bind(&created.body)
        .bind(&created.fields)
        .bind(revision_status)
        .bind(revision_scheduled_at)
        .bind(revision_published_at)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "content.created".to_owned(),
                    resource_type: "Content".to_owned(),
                    resource_id: Some(created.id.into_uuid()),
                    payload: serde_json::json!({"revision": created.revision}),
                },
            )
            .await?;

        Ok(created)
    }

    pub async fn list(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &ContentListFilter,
    ) -> Result<Page<Content>> {
        let kind = filter.kind.as_deref().map(ContentKind::parse).transpose()?;
        let language = filter
            .language
            .as_deref()
            .map(LanguageTag::parse)
            .transpose()?;
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());

        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, site_id, kind, language, slug, title, excerpt, body, fields, status, scheduled_at, published_at, revision, created_at, updated_at
               from content_entries where site_id = ",
        );
        query
            .push_bind(context.site_id.into_uuid())
            .push(" and deleted_at is null");

        if let Some(kind) = kind {
            query.push(" and kind = ").push_bind(kind.as_str());
        }
        if let Some(language) = language {
            query.push(" and language = ").push_bind(language.as_str());
        }
        if let Some(status) = filter.status {
            query.push(" and status = ").push_bind(status.as_str());
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

        let has_next = rows.len() > usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if has_next {
            let row = rows
                .get(
                    usize::try_from(limit)
                        .map_err(|_| MaviError::Internal)?
                        .saturating_sub(1),
                )
                .ok_or(MaviError::Internal)?;
            Some(encode_cursor(
                row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                row.try_get("id").map_err(|_| MaviError::Internal)?,
            )?)
        } else {
            None
        };

        let items = rows
            .into_iter()
            .take(usize::try_from(limit).map_err(|_| MaviError::Internal)?)
            .map(|row| from_row(&row))
            .collect::<Result<Vec<_>>>()?;

        Ok(Page::new(items, next_cursor))
    }

    pub async fn get(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ContentId,
    ) -> Result<Content> {
        self.get_with_lock(tx, context, id, false).await
    }

    async fn get_for_update(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ContentId,
    ) -> Result<Content> {
        self.get_with_lock(tx, context, id, true).await
    }

    async fn get_with_lock(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ContentId,
        lock: bool,
    ) -> Result<Content> {
        let query = if lock {
            "select id, site_id, kind, language, slug, title, excerpt, body, fields, status, scheduled_at, published_at, revision, created_at, updated_at
               from content_entries where site_id = $1 and id = $2 and deleted_at is null
               for update"
        } else {
            "select id, site_id, kind, language, slug, title, excerpt, body, fields, status, scheduled_at, published_at, revision, created_at, updated_at
               from content_entries where site_id = $1 and id = $2 and deleted_at is null"
        };
        let row = sqlx::query(query)
            .bind(context.site_id.into_uuid())
            .bind(id.into_uuid())
            .fetch_optional(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?
            .ok_or(MaviError::NotFound {
                resource: CONTENT_NOT_FOUND,
            })?;

        from_row(&row)
    }

    pub async fn public_get(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        language: &str,
        slug: &str,
    ) -> Result<Content> {
        let language = LanguageTag::parse(language)?;
        let slug = Slug::parse(slug)?;
        let row = sqlx::query(
            "select id, site_id, kind, language, slug, title, excerpt, body, fields, status, scheduled_at, published_at, revision, created_at, updated_at
              from content_entries
              where site_id = $1 and language = $2 and slug = $3
                and status = 'published' and published_at <= clock_timestamp() and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(language.as_str())
        .bind(slug.as_str())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if let Some(row) = row {
            return from_row(&row);
        }

        let historical_row = sqlx::query(
            "select c.id, c.site_id, c.kind, c.language, c.slug, c.title, c.excerpt, c.body, c.fields, c.status, c.scheduled_at, c.published_at, c.revision, c.created_at, c.updated_at
               from content_slug_history h
               join content_entries c on c.site_id = h.site_id and c.id = h.content_id
              where h.site_id = $1 and h.language = $2 and h.slug = $3
                and c.status = 'published' and c.published_at <= clock_timestamp() and c.deleted_at is null
              order by h.created_at desc
              limit 1",
        )
        .bind(context.site_id.into_uuid())
        .bind(language.as_str())
        .bind(slug.as_str())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: CONTENT_NOT_FOUND,
        })?;

        from_row(&historical_row)
    }

    /// Reads the first published entry available in the ordered language
    /// candidates supplied by the settings policy.
    pub async fn public_get_any(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        languages: &[String],
        slug: &str,
    ) -> Result<Content> {
        if languages.is_empty() {
            return Err(MaviError::NotFound {
                resource: CONTENT_NOT_FOUND,
            });
        }

        for language in languages {
            match self.public_get(tx, context, language, slug).await {
                Ok(entry) => return Ok(entry),
                Err(MaviError::NotFound { .. }) => {}
                Err(error) => return Err(error),
            }
        }

        Err(MaviError::NotFound {
            resource: CONTENT_NOT_FOUND,
        })
    }

    /// Lists published content assigned to a resolved public taxonomy term.
    ///
    /// The taxonomy service resolves the term and its language first. The
    /// content repository owns this read model so the HTTP layer does not
    /// join domain tables or construct a second archive-specific query.
    pub async fn public_list_for_term(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        term_id: TermId,
        language: &str,
        page: &PageRequest,
    ) -> Result<Page<Content>> {
        let language = LanguageTag::parse(language)?;
        let after = page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select c.id, c.site_id, c.kind, c.language, c.slug, c.title, c.excerpt,
                    c.body, c.fields, c.status, c.scheduled_at, c.published_at,
                    c.revision, c.created_at, c.updated_at
               from content_entries c
              where c.site_id = ",
        );
        query
            .push_bind(context.site_id.into_uuid())
            .push(" and c.language = ")
            .push_bind(language.as_str())
            .push(" and c.status = 'published'")
            .push(" and c.published_at <= clock_timestamp()")
            .push(" and c.deleted_at is null")
            .push(" and exists (")
            .push("select 1 from content_term_assignments a")
            .push(" where a.site_id = c.site_id")
            .push(" and a.content_id = c.id")
            .push(" and a.term_id = ")
            .push_bind(term_id.into_uuid())
            .push(")");

        if let Some(after) = after {
            query
                .push(" and (c.created_at, c.id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }

        let rows = query
            .push(" order by c.created_at desc, c.id desc limit ")
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
            Some(encode_cursor(
                row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                row.try_get("id").map_err(|_| MaviError::Internal)?,
            )?)
        } else {
            None
        };
        let items = rows
            .into_iter()
            .take(limit_usize)
            .map(|row| from_row(&row))
            .collect::<Result<Vec<_>>>()?;

        Ok(Page::new(items, next_cursor))
    }

    pub async fn update(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ContentId,
        input: &UpdateContent,
        now: DateTime<Utc>,
    ) -> Result<Content> {
        self.update_internal(tx, context, id, input, now, "content.updated")
            .await
    }

    /// Restores an immutable historical snapshot into a new draft revision.
    /// Publication is deliberately not copied from history: a caller must
    /// explicitly publish the restored content after reviewing it.
    pub async fn restore_revision(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ContentId,
        revision: u32,
        now: DateTime<Utc>,
    ) -> Result<Content> {
        let source = self.read_revision(tx, context, id, revision).await?;
        self.update_internal_with_source(
            tx,
            context,
            id,
            &UpdateContent {
                slug: Some(source.slug.as_str().to_owned()),
                title: Some(source.title.clone()),
                excerpt: Some(source.excerpt.clone()),
                body: Some(source.body.clone()),
                fields: Some(source.fields.clone()),
                publication: Some(PublicationInput::Draft),
            },
            now,
            UpdateAudit {
                action: "content.revision_restored",
                source_revision: Some(source.revision),
            },
        )
        .await
    }

    async fn update_internal(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ContentId,
        input: &UpdateContent,
        now: DateTime<Utc>,
        audit_action: &str,
    ) -> Result<Content> {
        self.update_internal_with_source(
            tx,
            context,
            id,
            input,
            now,
            UpdateAudit {
                action: audit_action,
                source_revision: None,
            },
        )
        .await
    }

    async fn update_internal_with_source(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ContentId,
        input: &UpdateContent,
        now: DateTime<Utc>,
        audit: UpdateAudit<'_>,
    ) -> Result<Content> {
        let current = self.get_for_update(tx, context, id).await?;
        let previous_slug = current.slug.as_str().to_owned();
        let slug = match &input.slug {
            Some(value) => Slug::parse(value)?,
            None => current.slug,
        };
        let title = match &input.title {
            Some(value) => Title::parse(value)?,
            None => Title::parse(&current.title)?,
        };
        let fields = input.fields.clone().unwrap_or(current.fields);
        validate_fields(&fields)?;
        content_types::validate_content_fields(tx, context, &current.kind, &fields).await?;
        let publication = match &input.publication {
            Some(value) => publication_input(value, now)?,
            None => current.publication,
        };
        let (status, scheduled_at, published_at) = publication.columns();
        let excerpt = input.excerpt.clone().unwrap_or(current.excerpt);
        let body = input.body.clone().unwrap_or(current.body);
        if body.chars().count() > 1_000_000 {
            return Err(MaviError::validation("content_body_too_large"));
        }
        let next_revision = current.revision.checked_add(1).ok_or(MaviError::Internal)?;

        if previous_slug != slug.as_str() {
            record_slug_history(tx, context, id, &current.language, &previous_slug).await?;
        }

        let row = sqlx::query(
            "update content_entries
                set slug = $3, title = $4, excerpt = $5, body = $6, fields = $7,
                    status = $8, scheduled_at = $9, published_at = $10,
                    revision = $11, updated_at = now()
              where site_id = $1 and id = $2 and deleted_at is null
             returning id, site_id, kind, language, slug, title, excerpt, body, fields, status, scheduled_at, published_at, revision, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(slug.as_str())
        .bind(title.as_str())
        .bind(excerpt)
        .bind(body)
        .bind(fields)
        .bind(status)
        .bind(scheduled_at)
        .bind(published_at)
        .bind(i32::try_from(next_revision).map_err(|_| MaviError::Internal)?)
        .fetch_optional(tx.conn())
        .await
        .map_err(|error| map_write_error(&error))?
        .ok_or(MaviError::NotFound {
            resource: CONTENT_NOT_FOUND,
        })?;

        let updated = from_row(&row)?;
        sqlx::query(
            "insert into content_revisions (site_id, content_id, revision, kind, language, slug, title, excerpt, body, fields, status, scheduled_at, published_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(context.site_id.into_uuid())
        .bind(updated.id.into_uuid())
        .bind(i32::try_from(updated.revision).map_err(|_| MaviError::Internal)?)
        .bind(updated.kind.as_str())
        .bind(updated.language.as_str())
        .bind(updated.slug.as_str())
        .bind(&updated.title)
        .bind(&updated.excerpt)
        .bind(&updated.body)
        .bind(&updated.fields)
        .bind(status)
        .bind(scheduled_at)
        .bind(published_at)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        let mut audit_payload = serde_json::Map::new();
        audit_payload.insert("revision".to_owned(), serde_json::json!(updated.revision));
        if let Some(source_revision) = audit.source_revision {
            audit_payload.insert(
                "source_revision".to_owned(),
                serde_json::json!(source_revision),
            );
        }

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: audit.action.to_owned(),
                    resource_type: "Content".to_owned(),
                    resource_id: Some(updated.id.into_uuid()),
                    payload: Value::Object(audit_payload),
                },
            )
            .await?;

        Ok(updated)
    }

    pub async fn publish(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ContentId,
        now: DateTime<Utc>,
    ) -> Result<Content> {
        self.update_internal(
            tx,
            context,
            id,
            &UpdateContent {
                publication: Some(PublicationInput::Publish),
                ..UpdateContent::default()
            },
            now,
            "content.published",
        )
        .await
    }

    pub async fn schedule(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ContentId,
        at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Content> {
        self.update_internal(
            tx,
            context,
            id,
            &UpdateContent {
                publication: Some(PublicationInput::Schedule(at)),
                ..UpdateContent::default()
            },
            now,
            "content.scheduled",
        )
        .await
    }

    /// Publishes a scheduled entry only when the job still describes its
    /// current schedule. The row lock makes the check and state transition
    /// one serialized operation, so a reschedule, edit or trash mutation can
    /// safely turn an old job into a no-op.
    pub async fn publish_scheduled(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ContentId,
        scheduled_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<ScheduledPublishOutcome> {
        let row = sqlx::query(
            "select id, site_id, kind, language, slug, title, excerpt, body, fields,
                    status, scheduled_at, published_at, revision, created_at, updated_at,
                    deleted_at
               from content_entries
              where site_id = $1 and id = $2
              for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        let Some(row) = row else {
            return Ok(ScheduledPublishOutcome::Skipped(
                ScheduledPublishSkipReason::Missing,
            ));
        };
        if row
            .try_get::<Option<DateTime<Utc>>, _>("deleted_at")
            .map_err(|_| MaviError::Internal)?
            .is_some()
        {
            return Ok(ScheduledPublishOutcome::Skipped(
                ScheduledPublishSkipReason::Deleted,
            ));
        }

        let current = from_row(&row)?;
        match current.publication {
            Publication::Scheduled { at } if at == scheduled_at => self
                .publish(tx, context, id, now)
                .await
                .map(|content| ScheduledPublishOutcome::Published(Box::new(content))),
            Publication::Scheduled { .. } => Ok(ScheduledPublishOutcome::Skipped(
                ScheduledPublishSkipReason::DifferentSchedule,
            )),
            Publication::Draft | Publication::Published { .. } | Publication::Archived => Ok(
                ScheduledPublishOutcome::Skipped(ScheduledPublishSkipReason::NoLongerScheduled),
            ),
        }
    }

    pub async fn archive(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ContentId,
        now: DateTime<Utc>,
    ) -> Result<Content> {
        self.update_internal(
            tx,
            context,
            id,
            &UpdateContent {
                publication: Some(PublicationInput::Archive),
                ..UpdateContent::default()
            },
            now,
            "content.archived",
        )
        .await
    }

    pub async fn trash(&self, tx: &mut SiteTx, context: &SiteContext, id: ContentId) -> Result<()> {
        sqlx::query(
            "update content_entries set deleted_at = now(), updated_at = now()
               where site_id = $1 and id = $2 and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)
        .and_then(|result| {
            if result.rows_affected() == 0 {
                Err(MaviError::NotFound {
                    resource: CONTENT_NOT_FOUND,
                })
            } else {
                Ok(())
            }
        })?;

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "content.trashed".to_owned(),
                    resource_type: "Content".to_owned(),
                    resource_id: Some(id.into_uuid()),
                    payload: Value::Object(serde_json::Map::new()),
                },
            )
            .await
    }

    pub async fn restore(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ContentId,
    ) -> Result<Content> {
        sqlx::query(
            "update content_entries set deleted_at = null, updated_at = now()
               where site_id = $1 and id = $2 and deleted_at is not null",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)
        .and_then(|result| {
            if result.rows_affected() == 0 {
                Err(MaviError::NotFound {
                    resource: CONTENT_NOT_FOUND,
                })
            } else {
                Ok(())
            }
        })?;

        let restored = self.get(tx, context, id).await?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "content.restored".to_owned(),
                    resource_type: "Content".to_owned(),
                    resource_id: Some(id.into_uuid()),
                    payload: Value::Object(serde_json::Map::new()),
                },
            )
            .await?;
        Ok(restored)
    }
}

fn map_write_error(error: &sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error
        && database.constraint() == Some("content_entries_site_language_slug")
    {
        return MaviError::conflict(CONTENT_SLUG_TAKEN);
    }

    MaviError::Internal
}

async fn record_slug_history(
    tx: &mut SiteTx,
    context: &SiteContext,
    content_id: ContentId,
    language: &LanguageTag,
    slug: &str,
) -> Result<()> {
    sqlx::query(
        "insert into content_slug_history (site_id, content_id, language, slug)
         values ($1, $2, $3, $4)
         on conflict (site_id, content_id, language, slug) do nothing",
    )
    .bind(context.site_id.into_uuid())
    .bind(content_id.into_uuid())
    .bind(language.as_str())
    .bind(slug)
    .execute(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    Ok(())
}

impl fmt::Display for ContentKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create() -> CreateContent {
        CreateContent {
            kind: "post".to_owned(),
            language: "en".to_owned(),
            slug: "hello-world".to_owned(),
            title: " Hello world ".to_owned(),
            excerpt: None,
            body: "body".to_owned(),
            fields: empty_fields(),
            publication: PublicationInput::Draft,
        }
    }

    #[test]
    fn validates_cms_identifiers_without_http_or_sql() {
        assert!(ContentKind::parse("post").is_ok());
        assert!(ContentKind::parse("Post").is_err());
        assert!(LanguageTag::parse("en-US").is_ok());
        assert!(LanguageTag::parse("e").is_err());
        assert!(LanguageTag::parse("1n").is_err());
        assert!(Slug::parse("hello-world").is_ok());
        assert!(Slug::parse("hello--world").is_err());
    }

    #[test]
    fn create_is_draft_by_default_and_trims_title() {
        let input = create();
        let checked = checked_new(&input, Utc::now()).expect("valid content");

        assert_eq!(checked.title.as_str(), "Hello world");
        assert_eq!(checked.publication, Publication::Draft);
    }

    #[test]
    fn publish_and_schedule_are_explicit_states() {
        let now = Utc::now();
        assert_eq!(
            publication_input(&PublicationInput::Publish, now).expect("publish"),
            Publication::Published { at: now }
        );
        assert!(matches!(
            publication_input(&PublicationInput::Schedule(now), now),
            Err(MaviError::Validation { code, field: None }) if code == CONTENT_STATE_INVALID
        ));
    }

    #[test]
    fn scheduled_publish_job_payload_is_strict_and_stable() {
        let content_id = ContentId::new();
        let scheduled_at = Utc::now() + chrono::Duration::hours(1);
        let payload = ScheduledPublishJob {
            content_id,
            scheduled_at,
        };
        let encoded = serde_json::to_value(&payload).expect("job payload");
        assert_eq!(encoded["content_id"], content_id.to_string());
        assert_eq!(
            encoded["scheduled_at"],
            scheduled_at.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
        );
        assert_eq!(SCHEDULED_PUBLISH_JOB.name, "content.publish_scheduled");
        assert_eq!(SCHEDULED_PUBLISH_JOB.max_attempts, 5);
        assert!(serde_json::from_value::<ScheduledPublishJob>(encoded).is_ok());
        assert!(serde_json::from_str::<ScheduledPublishJob>(
            r#"{"content_id":"00000000-0000-0000-0000-000000000000","scheduled_at":"2030-01-01T00:00:00Z","unexpected":true}"#
        )
        .is_err());
    }

    #[test]
    fn fields_must_be_an_object() {
        let mut input = create();
        input.fields = Value::Array(Vec::new());
        assert!(checked_new(&input, Utc::now()).is_err());
    }

    #[test]
    fn content_cursor_round_trips_and_rejects_corruption() {
        let id = Uuid::now_v7();
        let created_at = Utc::now();
        let cursor = encode_cursor(created_at, id).expect("cursor");
        let decoded = decode_cursor(&cursor).expect("decoded cursor");

        assert_eq!(decoded.id, id);
        assert_eq!(decoded.created_at, created_at);
        assert!(decode_cursor(&Cursor::parse("not-a-content-cursor").expect("cursor")).is_err());
    }

    #[test]
    fn archive_is_an_explicit_publication_state() {
        assert_eq!(
            publication_input(&PublicationInput::Archive, Utc::now()).expect("archive"),
            Publication::Archived
        );
    }

    #[test]
    fn canonical_content_api_is_self_consistent() {
        api().validate().expect("content API contract is valid");
    }
}
