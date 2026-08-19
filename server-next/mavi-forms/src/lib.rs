//! Site-scoped forms and validated public submissions.
//!
//! A form declaration is checked once by an authenticated editor. A public
//! submission is checked again against that declaration before any row is
//! written. Management lists use opaque keyset cursors, while public routes
//! reveal only the form shape or a submission receipt.

use std::{collections::BTreeSet, fmt::Debug};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Cursor, ErrorCode, FormId, FormSubmissionId, JobId, MaviError, Page,
    PageRequest, Result, SiteContext,
};
use mavi_jobs::{JobKind, JobsService};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::Row;
use uuid::Uuid;

mod relocation;

pub use relocation::{
    FORMS_RELOCATION_CONFLICT, FORMS_RELOCATION_FORMAT, FORMS_RELOCATION_VERSION, FormRelocation,
    FormSubmissionRelocation, FormsRelocation, MAX_FORMS_RELOCATION_BYTES,
    MAX_FORMS_RELOCATION_RECORDS,
};

pub const FORM_NOT_FOUND: &str = "form_not_found";
pub const FORM_SUBMISSION_NOT_FOUND: &str = "form_submission_not_found";
pub const FORM_SLUG_INVALID: &str = "form_slug_invalid";
pub const FORM_SLUG_TAKEN: &str = "form_slug_taken";
pub const FORM_NAME_INVALID: &str = "form_name_invalid";
pub const FORM_FIELDS_INVALID: &str = "form_fields_invalid";
pub const FORM_FIELD_KEY_INVALID: &str = "form_field_key_invalid";
pub const FORM_FIELD_LABEL_INVALID: &str = "form_field_label_invalid";
pub const FORM_FIELD_DUPLICATE: &str = "form_field_duplicate";
pub const FORM_FIELD_OPTIONS_INVALID: &str = "form_field_options_invalid";
pub const FORM_KEPT_DAYS_INVALID: &str = "form_kept_days_invalid";
pub const FORM_ANSWER_REQUIRED: &str = "form_answer_required";
pub const FORM_ANSWER_TYPE_INVALID: &str = "form_answer_type_invalid";
pub const FORM_ANSWER_UNKNOWN: &str = "form_answer_unknown";
pub const FORM_ANSWERS_TOO_LARGE: &str = "form_answers_too_large";
pub const FORM_SUBMISSION_CLOSED: &str = "form_submission_closed";
pub const FORM_RETENTION_JOB: JobKind = JobKind::new("forms.retention", 5);
pub const FORM_RETENTION_BUCKET_SECONDS: i64 = 24 * 60 * 60;

pub const DEFAULT_KEPT_DAYS: i32 = 365;
pub const MAX_FORM_FIELDS: usize = 50;
pub const MAX_FORM_OPTIONS: usize = 100;
pub const MAX_FORM_ANSWERS_BYTES: usize = 64 * 1024;
const MAX_FORM_SLUG_CHARS: usize = 160;
const MAX_FORM_NAME_CHARS: usize = 200;
const MAX_FIELD_KEY_CHARS: usize = 64;
const MAX_FIELD_LABEL_CHARS: usize = 200;
const MAX_OPTION_CHARS: usize = 160;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormFieldKind {
    #[default]
    Text,
    Long,
    Email,
    Number,
    Choice,
    Boolean,
}

impl FormFieldKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Long => "long",
            Self::Email => "email",
            Self::Number => "number",
            Self::Choice => "choice",
            Self::Boolean => "boolean",
        }
    }

    fn expects_choice(self) -> bool {
        matches!(self, Self::Choice)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormField {
    pub key: String,
    pub label: String,
    pub required: bool,
    #[serde(default)]
    pub kind: FormFieldKind,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreateForm {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub fields: Vec<FormField>,
    #[serde(default)]
    pub kept_days: Option<i32>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UpdateForm {
    pub name: Option<String>,
    pub fields: Option<Vec<FormField>>,
    pub open: Option<bool>,
    pub kept_days: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SubmitForm {
    pub answers: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FormListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SubmissionListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    #[serde(default)]
    pub unread: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Form {
    pub id: FormId,
    pub slug: String,
    pub name: String,
    pub fields: Vec<FormField>,
    pub open: bool,
    pub kept_days: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PublicForm {
    pub slug: String,
    pub name: String,
    pub fields: Vec<FormField>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FormSubmission {
    pub id: FormSubmissionId,
    pub form_id: FormId,
    pub answers: Map<String, Value>,
    pub seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SubmissionReceipt {
    pub id: FormSubmissionId,
}

/// The payload for the idempotent daily form-submission retention job.
///
/// The bucket is deliberately part of the payload and idempotency key. A
/// worker may poll many times per day, while a restarted worker can safely
/// claim the same day's cleanup without creating another job.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormRetentionJob {
    pub bucket: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SeenCount {
    pub seen: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RecentCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes =
        serde_json::to_vec(&RecentCursor { created_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor(cursor: &Cursor) -> Result<RecentCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FormService;

#[must_use]
pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new(endpoints()).with_shapes(shapes())
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn endpoints() -> Vec<Endpoint> {
    let view = Permission {
        capability: Capability::Forms,
        action: Action::View,
    };
    let write = Permission {
        capability: Capability::Forms,
        action: Action::Write,
    };
    let delete = Permission {
        capability: Capability::Forms,
        action: Action::Delete,
    };

    vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/forms",
            "forms.list",
            "List site forms with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("FormListFilter")
        .returns(200, "FormPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/forms",
            "forms.create",
            "Create a validated site form",
        )
        .account_or_assistant()
        .requires(write)
        .takes("CreateForm")
        .returns(201, "Form")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/forms/{id}",
            "forms.read",
            "Read one site form",
        )
        .account_or_assistant()
        .requires(view)
        .returns(200, "Form")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/forms/{id}",
            "forms.update",
            "Update a form declaration or open state",
        )
        .account_or_assistant()
        .requires(write)
        .takes("UpdateForm")
        .returns(200, "Form")
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
            "/api/v1/forms/{id}",
            "forms.delete",
            "Move a form out of the active form catalog",
        )
        .account_or_assistant()
        .requires(delete)
        .returns(204, "Empty")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/forms/{id}/submissions",
            "forms.submissions.list",
            "List form submissions with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("SubmissionListFilter")
        .returns(200, "SubmissionPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/forms/{id}/submissions/mark-read",
            "forms.submissions.mark_read",
            "Mark submissions received up to this transaction as read",
        )
        .account_or_assistant()
        .requires(write)
        .returns(200, "SeenCount")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/form-submissions/{id}",
            "forms.submissions.delete",
            "Forget one form submission",
        )
        .account_or_assistant()
        .requires(delete)
        .returns(204, "Empty")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/public/v1/forms/{slug}",
            "forms.public.read",
            "Read an open public form without site management metadata",
        )
        .public()
        .returns(200, "PublicForm")
        .refuses([
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/public/v1/forms/{slug}/submissions",
            "forms.public.submit",
            "Validate and accept one public form submission",
        )
        .public_mutation()
        .takes("SubmitForm")
        .returns(201, "SubmissionReceipt")
        .refuses([
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
    ]
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "FormFieldKind",
            json!({"type": "string", "enum": ["text", "long", "email", "number", "choice", "boolean"]}),
        ),
        Shape::new(
            "FormField",
            json!({
                "type": "object",
                "required": ["key", "label", "required", "kind", "options"],
                "additionalProperties": false,
                "properties": {
                    "key": {"type": "string", "minLength": 1, "maxLength": MAX_FIELD_KEY_CHARS},
                    "label": {"type": "string", "minLength": 1, "maxLength": MAX_FIELD_LABEL_CHARS},
                    "required": {"type": "boolean"},
                    "kind": {"$ref": "#/components/schemas/FormFieldKind"},
                    "options": {"type": "array", "maxItems": MAX_FORM_OPTIONS, "items": {"type": "string", "maxLength": MAX_OPTION_CHARS}}
                }
            }),
        ),
        Shape::new(
            "CreateForm",
            json!({
                "type": "object",
                "required": ["slug", "name"],
                "additionalProperties": false,
                "properties": {
                    "slug": {"type": "string", "minLength": 1, "maxLength": MAX_FORM_SLUG_CHARS},
                    "name": {"type": "string", "minLength": 1, "maxLength": MAX_FORM_NAME_CHARS},
                    "fields": {"type": "array", "maxItems": MAX_FORM_FIELDS, "items": {"$ref": "#/components/schemas/FormField"}},
                    "kept_days": {"type": ["integer", "null"], "minimum": 1, "maximum": 3650}
                }
            }),
        ),
        Shape::new(
            "UpdateForm",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "name": {"type": ["string", "null"], "maxLength": MAX_FORM_NAME_CHARS},
                    "fields": {"type": ["array", "null"], "maxItems": MAX_FORM_FIELDS, "items": {"$ref": "#/components/schemas/FormField"}},
                    "open": {"type": ["boolean", "null"]},
                    "kept_days": {"type": ["integer", "null"], "minimum": 1, "maximum": 3650}
                }
            }),
        ),
        Shape::new(
            "SubmitForm",
            json!({
                "type": "object",
                "required": ["answers"],
                "additionalProperties": false,
                "properties": {"answers": {"type": "object", "additionalProperties": true}}
            }),
        ),
        Shape::new(
            "FormListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }}),
        ),
        Shape::new(
            "SubmissionListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                "unread": {"type": "boolean"}
            }}),
        ),
        Shape::new(
            "Form",
            json!({
                "type": "object",
                "required": ["id", "slug", "name", "fields", "open", "kept_days", "created_at", "updated_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "slug": {"type": "string"},
                    "name": {"type": "string"},
                    "fields": {"type": "array", "items": {"$ref": "#/components/schemas/FormField"}},
                    "open": {"type": "boolean"},
                    "kept_days": {"type": "integer", "minimum": 1, "maximum": 3650},
                    "created_at": {"type": "string", "format": "date-time"},
                    "updated_at": {"type": "string", "format": "date-time"}
                }
            }),
        ),
        Shape::new(
            "PublicForm",
            json!({
                "type": "object",
                "required": ["slug", "name", "fields"],
                "properties": {
                    "slug": {"type": "string"},
                    "name": {"type": "string"},
                    "fields": {"type": "array", "items": {"$ref": "#/components/schemas/FormField"}}
                }
            }),
        ),
        Shape::new(
            "FormSubmission",
            json!({
                "type": "object",
                "required": ["id", "form_id", "answers", "seen_at", "created_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "form_id": {"type": "string", "format": "uuid"},
                    "answers": {"type": "object", "additionalProperties": true},
                    "seen_at": {"type": ["string", "null"], "format": "date-time"},
                    "created_at": {"type": "string", "format": "date-time"}
                }
            }),
        ),
        Shape::new(
            "SubmissionReceipt",
            json!({"type": "object", "required": ["id"], "properties": {"id": {"type": "string", "format": "uuid"}}}),
        ),
        Shape::new(
            "SeenCount",
            json!({"type": "object", "required": ["seen"], "properties": {"seen": {"type": "integer", "format": "int64", "minimum": 0}}}),
        ),
        Shape::new(
            "FormPage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/Form"}},
                "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
        Shape::new(
            "SubmissionPage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/FormSubmission"}},
                "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
    ]
}

impl FormService {
    /// Enqueues one retention pass for the current UTC day in the same
    /// site-scoped transaction as any other worker discovery work.
    pub async fn enqueue_retention_job(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        jobs: &JobsService,
        now: DateTime<Utc>,
    ) -> Result<JobId> {
        let bucket = now.timestamp().div_euclid(FORM_RETENTION_BUCKET_SECONDS);
        let payload =
            serde_json::to_value(FormRetentionJob { bucket }).map_err(|_| MaviError::Internal)?;
        let idempotency_key = format!("forms:retention:{}:{}", context.site_id, bucket);
        jobs.enqueue(
            tx,
            context,
            FORM_RETENTION_JOB.name,
            &payload,
            None,
            Some(&idempotency_key),
        )
        .await
    }

    /// Marks submissions older than their form's retention period as deleted.
    ///
    /// Retention keeps only a tombstone: answers are redacted and normal inbox
    /// reads stop exposing the submission immediately. The audit receipt is
    /// written in the same transaction as the update, so a failed worker claim
    /// cannot report a cleanup that did not commit.
    pub async fn prune_expired_submissions(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        now: DateTime<Utc>,
        bucket: i64,
    ) -> Result<u64> {
        if bucket < 0 {
            return Err(MaviError::validation("form_retention_bucket_invalid"));
        }
        let result = sqlx::query(
            "update form_submissions as submission
                set answers = '{}'::jsonb,
                    deleted_at = $1
               from forms as form
              where submission.site_id = $2
                and form.site_id = $2
                and submission.form_id = form.id
                and submission.deleted_at is null
                and submission.created_at < $1 - (form.kept_days * interval '1 day')",
        )
        .bind(now)
        .bind(context.site_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let deleted = result.rows_affected();
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "forms.submissions.retention_pruned".to_owned(),
                    resource_type: "FormSubmissionRetention".to_owned(),
                    resource_id: None,
                    payload: json!({"bucket": bucket, "deleted": deleted}),
                },
            )
            .await?;
        Ok(deleted)
    }

    pub async fn list(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &FormListFilter,
    ) -> Result<Page<Form>> {
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, slug, name, fields, open, kept_days, created_at, updated_at
               from forms where site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        query.push(" and deleted_at is null");
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
        let mut items = rows.iter().map(from_form_row).collect::<Result<Vec<_>>>()?;
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

    pub async fn create(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateForm,
    ) -> Result<Form> {
        let slug = validate_slug(&input.slug)?;
        let name = validate_name(&input.name)?;
        let fields = validate_fields(&input.fields)?;
        let kept_days = validate_kept_days(input.kept_days.unwrap_or(DEFAULT_KEPT_DAYS))?;
        let id = FormId::new();
        let row = sqlx::query(
            "insert into forms (site_id, id, slug, name, fields, kept_days)
             values ($1, $2, $3, $4, $5, $6)
             returning id, slug, name, fields, open, kept_days, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(&slug)
        .bind(&name)
        .bind(serde_json::to_value(&fields).map_err(|_| MaviError::Internal)?)
        .bind(kept_days)
        .fetch_one(tx.conn())
        .await
        .map_err(|error| map_form_write_error(&error))?;
        let form = from_form_row(&row)?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "forms.form.created".to_owned(),
                    resource_type: "Form".to_owned(),
                    resource_id: Some(id.into_uuid()),
                    payload: json!({"slug": form.slug, "field_count": form.fields.len()}),
                },
            )
            .await?;
        Ok(form)
    }

    pub async fn get(&self, tx: &mut SiteTx, context: &SiteContext, id: FormId) -> Result<Form> {
        let row = sqlx::query(
            "select id, slug, name, fields, open, kept_days, created_at, updated_at
               from forms where site_id = $1 and id = $2 and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: FORM_NOT_FOUND,
        })?;
        from_form_row(&row)
    }

    pub async fn update(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: FormId,
        input: &UpdateForm,
    ) -> Result<Form> {
        let name = input.name.as_deref().map(validate_name).transpose()?;
        let fields = input
            .fields
            .as_ref()
            .map(|fields| validate_fields(fields))
            .transpose()?;
        let kept_days = input.kept_days.map(validate_kept_days).transpose()?;
        if name.is_none() && fields.is_none() && input.open.is_none() && kept_days.is_none() {
            return self.get(tx, context, id).await;
        }
        let row = sqlx::query(
            "update forms
                set name = coalesce($3, name),
                    fields = coalesce($4, fields),
                    open = coalesce($5, open),
                    kept_days = coalesce($6, kept_days),
                    updated_at = clock_timestamp()
              where site_id = $1 and id = $2 and deleted_at is null
             returning id, slug, name, fields, open, kept_days, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(name.as_deref())
        .bind(
            fields
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(|_| MaviError::Internal)?,
        )
        .bind(input.open)
        .bind(kept_days)
        .fetch_optional(tx.conn())
        .await
        .map_err(|error| map_form_write_error(&error))?
        .ok_or(MaviError::NotFound {
            resource: FORM_NOT_FOUND,
        })?;
        let form = from_form_row(&row)?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "forms.form.updated".to_owned(),
                    resource_type: "Form".to_owned(),
                    resource_id: Some(id.into_uuid()),
                    payload: json!({"slug": form.slug, "open": form.open}),
                },
            )
            .await?;
        Ok(form)
    }

    pub async fn delete(&self, tx: &mut SiteTx, context: &SiteContext, id: FormId) -> Result<()> {
        let result = sqlx::query(
            "update forms set deleted_at = clock_timestamp(), updated_at = clock_timestamp()
              where site_id = $1 and id = $2 and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if result.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: FORM_NOT_FOUND,
            });
        }
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "forms.form.deleted".to_owned(),
                    resource_type: "Form".to_owned(),
                    resource_id: Some(id.into_uuid()),
                    payload: json!({}),
                },
            )
            .await
    }

    pub async fn public_get(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        slug: &str,
    ) -> Result<PublicForm> {
        let slug = validate_slug(slug)?;
        let row = sqlx::query(
            "select slug, name, fields from forms
              where site_id = $1 and slug = $2 and open and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(slug)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: FORM_NOT_FOUND,
        })?;
        Ok(PublicForm {
            slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
            name: row.try_get("name").map_err(|_| MaviError::Internal)?,
            fields: decode_fields(
                &row.try_get::<Value, _>("fields")
                    .map_err(|_| MaviError::Internal)?,
            )?,
        })
    }

    pub async fn submit(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        slug: &str,
        input: &SubmitForm,
    ) -> Result<SubmissionReceipt> {
        let slug = validate_slug(slug)?;
        let row = sqlx::query(
            "select id, fields from forms
              where site_id = $1 and slug = $2 and open and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(slug)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: FORM_NOT_FOUND,
        })?;
        let form_id = FormId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
        let fields = decode_fields(
            &row.try_get::<Value, _>("fields")
                .map_err(|_| MaviError::Internal)?,
        )?;
        validate_answers(&input.answers, &fields)?;
        let id = FormSubmissionId::new();
        sqlx::query(
            "insert into form_submissions (site_id, id, form_id, answers)
             values ($1, $2, $3, $4)",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(form_id.into_uuid())
        .bind(Value::Object(input.answers.clone()))
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "forms.submission.received".to_owned(),
                    resource_type: "FormSubmission".to_owned(),
                    resource_id: Some(id.into_uuid()),
                    payload: json!({"form_id": form_id}),
                },
            )
            .await?;
        Ok(SubmissionReceipt { id })
    }

    pub async fn list_submissions(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        form_id: FormId,
        filter: &SubmissionListFilter,
    ) -> Result<Page<FormSubmission>> {
        self.get(tx, context, form_id).await?;
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, form_id, answers, seen_at, created_at
               from form_submissions where site_id = ",
        );
        query
            .push_bind(context.site_id.into_uuid())
            .push(" and form_id = ")
            .push_bind(form_id.into_uuid())
            .push(" and deleted_at is null");
        if filter.unread {
            query.push(" and seen_at is null");
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
        let mut items = rows
            .iter()
            .map(from_submission_row)
            .collect::<Result<Vec<_>>>()?;
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

    pub async fn mark_read(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        form_id: FormId,
    ) -> Result<SeenCount> {
        self.get(tx, context, form_id).await?;
        let result = sqlx::query(
            "update form_submissions
                set seen_at = clock_timestamp()
              where site_id = $1 and form_id = $2 and deleted_at is null
                and seen_at is null and created_at <= clock_timestamp()",
        )
        .bind(context.site_id.into_uuid())
        .bind(form_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let seen = result.rows_affected();
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "forms.submission.marked_read".to_owned(),
                    resource_type: "Form".to_owned(),
                    resource_id: Some(form_id.into_uuid()),
                    payload: json!({"seen": seen}),
                },
            )
            .await?;
        Ok(SeenCount { seen })
    }

    pub async fn delete_submission(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: FormSubmissionId,
    ) -> Result<()> {
        let row = sqlx::query(
            "update form_submissions
                set answers = '{}'::jsonb,
                    deleted_at = clock_timestamp()
              where site_id = $1 and id = $2 and deleted_at is null
             returning form_id",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: FORM_SUBMISSION_NOT_FOUND,
        })?;
        let form_id: Uuid = row.try_get("form_id").map_err(|_| MaviError::Internal)?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "forms.submission.deleted".to_owned(),
                    resource_type: "FormSubmission".to_owned(),
                    resource_id: Some(id.into_uuid()),
                    payload: json!({"form_id": form_id}),
                },
            )
            .await
    }
}

fn from_form_row(row: &sqlx::postgres::PgRow) -> Result<Form> {
    Ok(Form {
        id: FormId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        fields: decode_fields(
            &row.try_get::<Value, _>("fields")
                .map_err(|_| MaviError::Internal)?,
        )?,
        open: row.try_get("open").map_err(|_| MaviError::Internal)?,
        kept_days: row.try_get("kept_days").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn from_submission_row(row: &sqlx::postgres::PgRow) -> Result<FormSubmission> {
    let answers: Value = row.try_get("answers").map_err(|_| MaviError::Internal)?;
    let answers = answers.as_object().cloned().ok_or(MaviError::Internal)?;
    Ok(FormSubmission {
        id: FormSubmissionId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        form_id: FormId::from_uuid(row.try_get("form_id").map_err(|_| MaviError::Internal)?),
        answers,
        seen_at: row.try_get("seen_at").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
    })
}

fn map_form_write_error(error: &sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error
        && database.constraint() == Some("forms_site_slug_active")
    {
        return MaviError::conflict(FORM_SLUG_TAKEN);
    }
    MaviError::Internal
}

fn validate_slug(value: &str) -> Result<String> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.chars().count() <= MAX_FORM_SLUG_CHARS
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        });
    if !valid {
        return Err(MaviError::validation_field(FORM_SLUG_INVALID, "slug"));
    }
    Ok(value.to_owned())
}

fn validate_name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_FORM_NAME_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(MaviError::validation_field(FORM_NAME_INVALID, "name"));
    }
    Ok(value.to_owned())
}

fn validate_kept_days(value: i32) -> Result<i32> {
    if (1..=3650).contains(&value) {
        Ok(value)
    } else {
        Err(MaviError::validation_field(
            FORM_KEPT_DAYS_INVALID,
            "kept_days",
        ))
    }
}

fn validate_fields(fields: &[FormField]) -> Result<Vec<FormField>> {
    if fields.len() > MAX_FORM_FIELDS {
        return Err(MaviError::validation(FORM_FIELDS_INVALID));
    }
    let mut keys = BTreeSet::new();
    let mut checked = Vec::with_capacity(fields.len());
    for field in fields {
        let key = field.key.trim();
        if key.is_empty()
            || key.chars().count() > MAX_FIELD_KEY_CHARS
            || !key.chars().enumerate().all(|(index, character)| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || (index > 0 && character == '_')
                    || (index > 0 && character == '-')
            })
            || !keys.insert(key.to_owned())
        {
            return Err(MaviError::validation_field(
                if keys.contains(key) {
                    FORM_FIELD_DUPLICATE
                } else {
                    FORM_FIELD_KEY_INVALID
                },
                "fields",
            ));
        }
        let label = field.label.trim();
        if label.is_empty()
            || label.chars().count() > MAX_FIELD_LABEL_CHARS
            || label.chars().any(char::is_control)
        {
            return Err(MaviError::validation_field(
                FORM_FIELD_LABEL_INVALID,
                "fields",
            ));
        }
        let options = field
            .options
            .iter()
            .map(|option| option.trim().to_owned())
            .collect::<Vec<_>>();
        if field.kind.expects_choice() {
            if options.is_empty()
                || options.len() > MAX_FORM_OPTIONS
                || options.iter().any(|option| {
                    option.is_empty()
                        || option.chars().count() > MAX_OPTION_CHARS
                        || option.chars().any(char::is_control)
                })
                || options.windows(2).any(|pair| pair[0] == pair[1])
            {
                return Err(MaviError::validation_field(
                    FORM_FIELD_OPTIONS_INVALID,
                    "fields",
                ));
            }
        } else if !options.is_empty() {
            return Err(MaviError::validation_field(
                FORM_FIELD_OPTIONS_INVALID,
                "fields",
            ));
        }
        checked.push(FormField {
            key: key.to_owned(),
            label: label.to_owned(),
            required: field.required,
            kind: field.kind,
            options,
        });
    }
    Ok(checked)
}

fn decode_fields(value: &Value) -> Result<Vec<FormField>> {
    let fields =
        serde_json::from_value::<Vec<FormField>>(value.clone()).map_err(|_| MaviError::Internal)?;
    validate_fields(&fields).map_err(|_| MaviError::Internal)
}

fn validate_answers(answers: &Map<String, Value>, fields: &[FormField]) -> Result<()> {
    let bytes = serde_json::to_vec(answers).map_err(|_| MaviError::Internal)?;
    if bytes.len() > MAX_FORM_ANSWERS_BYTES {
        return Err(MaviError::validation(FORM_ANSWERS_TOO_LARGE));
    }
    for field in fields {
        let value = answers.get(&field.key);
        let empty = value.is_none_or(Value::is_null)
            || value.is_some_and(|value| value.as_str().is_some_and(|text| text.trim().is_empty()));
        if empty {
            if field.required {
                return Err(MaviError::validation_field(
                    FORM_ANSWER_REQUIRED,
                    format!("answers.{}", field.key),
                ));
            }
            continue;
        }
        let value = value.expect("non-empty answer exists");
        let valid = match field.kind {
            FormFieldKind::Text | FormFieldKind::Long => value.is_string(),
            FormFieldKind::Email => value.as_str().is_some_and(valid_email),
            FormFieldKind::Number => value.is_number(),
            FormFieldKind::Choice => value
                .as_str()
                .is_some_and(|value| field.options.iter().any(|option| option == value)),
            FormFieldKind::Boolean => value.is_boolean(),
        };
        if !valid {
            return Err(MaviError::validation_field(
                FORM_ANSWER_TYPE_INVALID,
                format!("answers.{}", field.key),
            ));
        }
    }
    if let Some(key) = answers
        .keys()
        .find(|key| !fields.iter().any(|field| field.key == **key))
    {
        return Err(MaviError::validation_field(
            FORM_ANSWER_UNKNOWN,
            format!("answers.{key}"),
        ));
    }
    Ok(())
}

fn valid_email(value: &str) -> bool {
    let value = value.trim();
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !value.chars().any(char::is_control)
        && !local.contains(' ')
        && domain.split('.').all(|part| {
            !part.is_empty()
                && part.chars().all(|character| {
                    character.is_ascii_alphanumeric() || character == '-' || character == '_'
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(key: &str, kind: FormFieldKind, required: bool) -> FormField {
        FormField {
            key: key.to_owned(),
            label: key.to_owned(),
            required,
            kind,
            options: Vec::new(),
        }
    }

    #[test]
    fn declarations_are_bounded_and_choices_are_explicit() {
        let mut choice = field("colour", FormFieldKind::Choice, true);
        assert!(validate_fields(&[choice.clone()]).is_err());
        choice.options = vec!["red".to_owned(), "blue".to_owned()];
        assert!(validate_fields(&[choice]).is_ok());

        let duplicate = vec![
            field("name", FormFieldKind::Text, false),
            field("name", FormFieldKind::Long, false),
        ];
        assert!(
            matches!(validate_fields(&duplicate), Err(MaviError::Validation { code, .. }) if code == FORM_FIELD_DUPLICATE)
        );
    }

    #[test]
    fn submission_validation_rejects_missing_unknown_and_wrong_answers() {
        let fields = vec![
            field("name", FormFieldKind::Text, true),
            field("email", FormFieldKind::Email, true),
        ];
        let valid = serde_json::from_value::<Map<String, Value>>(json!({
            "name": "Visitor",
            "email": "visitor@example.test"
        }))
        .expect("answers");
        assert!(validate_answers(&valid, &fields).is_ok());
        let missing = serde_json::from_value::<Map<String, Value>>(json!({"name": "Visitor"}))
            .expect("answers");
        assert!(
            matches!(validate_answers(&missing, &fields), Err(MaviError::Validation { code, .. }) if code == FORM_ANSWER_REQUIRED)
        );
        let unknown = serde_json::from_value::<Map<String, Value>>(json!({
            "name": "Visitor",
            "email": "visitor@example.test",
            "role": "owner"
        }))
        .expect("answers");
        assert!(
            matches!(validate_answers(&unknown, &fields), Err(MaviError::Validation { code, .. }) if code == FORM_ANSWER_UNKNOWN)
        );
    }

    #[test]
    fn forms_use_only_opaque_keyset_pagination() {
        let catalog = api();
        catalog.validate().expect("forms API contract");
        for name in ["FormListFilter", "SubmissionListFilter"] {
            let shape = shapes()
                .into_iter()
                .find(|shape| shape.name == name)
                .expect("filter shape");
            let properties = shape.schema["properties"].as_object().expect("properties");
            assert!(properties.contains_key("after"));
            assert!(properties.contains_key("limit"));
            assert!(!properties.contains_key("page"));
            assert!(!properties.contains_key("offset"));
        }
    }
}
