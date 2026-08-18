use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_core::{MaviError, Result, SiteContext, SiteId};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::Row;
use uuid::Uuid;

use super::{FormField, FormService, decode_fields, validate_fields};

pub const FORMS_RELOCATION_FORMAT: &str = "mavi.forms.relocation";
pub const FORMS_RELOCATION_VERSION: u16 = 1;
pub const MAX_FORMS_RELOCATION_RECORDS: usize = 20_000;
pub const MAX_FORMS_RELOCATION_BYTES: usize = 128 * 1024 * 1024;

/// Authenticated shard relocation data for the forms domain.
///
/// This is intentionally separate from the public portable bundle. Forms and
/// submissions can contain personal data, and deleted rows are needed to keep
/// the trusted target's restore/trash semantics complete.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormsRelocation {
    pub format: String,
    pub version: u16,
    pub source_site_id: SiteId,
    pub forms: Vec<FormRelocation>,
    pub submissions: Vec<FormSubmissionRelocation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormRelocation {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub fields: Vec<FormField>,
    pub open: bool,
    pub kept_days: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormSubmissionRelocation {
    pub id: Uuid,
    pub form_id: Uuid,
    pub answers: Map<String, Value>,
    pub seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl FormsRelocation {
    #[must_use]
    pub fn empty(source_site_id: SiteId) -> Self {
        Self {
            format: FORMS_RELOCATION_FORMAT.to_owned(),
            version: FORMS_RELOCATION_VERSION,
            source_site_id,
            forms: Vec::new(),
            submissions: Vec::new(),
        }
    }

    pub fn validate_for_relocation(&self, target_site: SiteId) -> Result<()> {
        if self.format != FORMS_RELOCATION_FORMAT {
            return Err(MaviError::validation("forms_relocation_format_invalid"));
        }
        if self.version != FORMS_RELOCATION_VERSION {
            return Err(MaviError::validation(
                "forms_relocation_version_unsupported",
            ));
        }
        if self.source_site_id != target_site || self.source_site_id.into_uuid().is_nil() {
            return Err(MaviError::conflict("forms_relocation_site_mismatch"));
        }
        if self
            .forms
            .len()
            .checked_add(self.submissions.len())
            .is_none_or(|count| count > MAX_FORMS_RELOCATION_RECORDS)
        {
            return Err(MaviError::validation("forms_relocation_counts_invalid"));
        }

        let mut form_ids = BTreeSet::new();
        for form in &self.forms {
            if form.id.is_nil()
                || !form_ids.insert(form.id)
                || !valid_form_slug(&form.slug)
                || !valid_form_name(&form.name)
                || validate_fields(&form.fields).is_err()
                || !(1..=3650).contains(&form.kept_days)
            {
                return Err(MaviError::validation("forms_relocation_form_invalid"));
            }
        }

        let mut submission_ids = BTreeSet::new();
        for submission in &self.submissions {
            if !form_ids.contains(&submission.form_id) {
                return Err(MaviError::validation(
                    "forms_relocation_submission_form_missing",
                ));
            }
            if submission.id.is_nil()
                || !submission_ids.insert(submission.id)
                || !valid_answers(&submission.answers)
            {
                return Err(MaviError::validation("forms_relocation_submission_invalid"));
            }
        }

        let bytes = serde_json::to_vec(self).map_err(|_| MaviError::Internal)?;
        if bytes.len() > MAX_FORMS_RELOCATION_BYTES {
            return Err(MaviError::validation("forms_relocation_too_large"));
        }
        Ok(())
    }

    pub fn record_count(&self) -> Result<i64> {
        let count = self
            .forms
            .len()
            .checked_add(self.submissions.len())
            .ok_or(MaviError::validation("forms_relocation_count_overflow"))?;
        i64::try_from(count).map_err(|_| MaviError::validation("forms_relocation_count_overflow"))
    }
}

impl FormService {
    #[allow(clippy::too_many_lines)]
    pub async fn export_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
    ) -> Result<FormsRelocation> {
        let forms = sqlx::query(
            "select id, slug, name, fields, open, kept_days, created_at, updated_at, deleted_at
               from forms where site_id = $1 order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            let fields: Value = row.try_get("fields").map_err(|_| MaviError::Internal)?;
            Ok(FormRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                fields: decode_fields(&fields)?,
                open: row.try_get("open").map_err(|_| MaviError::Internal)?,
                kept_days: row.try_get("kept_days").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let submissions = sqlx::query(
            "select id, form_id, answers, seen_at, created_at, deleted_at
               from form_submissions where site_id = $1 order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            let answers: Value = row.try_get("answers").map_err(|_| MaviError::Internal)?;
            let answers = answers.as_object().cloned().ok_or(MaviError::Internal)?;
            Ok(FormSubmissionRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                form_id: row.try_get("form_id").map_err(|_| MaviError::Internal)?,
                answers,
                seen_at: row.try_get("seen_at").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let relocation = FormsRelocation {
            format: FORMS_RELOCATION_FORMAT.to_owned(),
            version: FORMS_RELOCATION_VERSION,
            source_site_id: context.site_id,
            forms,
            submissions,
        };
        relocation.validate_for_relocation(context.site_id)?;
        Ok(relocation)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn import_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        relocation: &FormsRelocation,
    ) -> Result<()> {
        relocation.validate_for_relocation(context.site_id)?;

        for form in &relocation.forms {
            sqlx::query(
                "insert into forms
                    (site_id, id, slug, name, fields, open, kept_days, created_at, updated_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 on conflict (site_id, id) do update set
                    slug = excluded.slug, name = excluded.name, fields = excluded.fields,
                    open = excluded.open, kept_days = excluded.kept_days,
                    created_at = excluded.created_at, updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(form.id)
            .bind(&form.slug)
            .bind(&form.name)
            .bind(serde_json::to_value(&form.fields).map_err(|_| MaviError::Internal)?)
            .bind(form.open)
            .bind(form.kept_days)
            .bind(form.created_at)
            .bind(form.updated_at)
            .bind(form.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        for submission in &relocation.submissions {
            sqlx::query(
                "insert into form_submissions
                    (site_id, id, form_id, answers, seen_at, created_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7)
                 on conflict (site_id, id) do update set
                    form_id = excluded.form_id, answers = excluded.answers,
                    seen_at = excluded.seen_at, created_at = excluded.created_at,
                    deleted_at = excluded.deleted_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(submission.id)
            .bind(submission.form_id)
            .bind(Value::Object(submission.answers.clone()))
            .bind(submission.seen_at)
            .bind(submission.created_at)
            .bind(submission.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "portable.forms.relocated".to_owned(),
                    resource_type: "FormsRelocation".to_owned(),
                    resource_id: None,
                    payload: json!({
                        "forms": relocation.forms.len(),
                        "submissions": relocation.submissions.len(),
                    }),
                },
            )
            .await
    }
}

fn valid_form_slug(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 160
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn valid_form_name(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= 200 && !value.chars().any(char::is_control)
}

fn valid_answers(answers: &Map<String, Value>) -> bool {
    serde_json::to_vec(answers).is_ok_and(|bytes| bytes.len() <= super::MAX_FORM_ANSWERS_BYTES)
}

fn map_write_error(error: &sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = &error
        && database.constraint() == Some("forms_site_slug_active")
    {
        return MaviError::conflict(FORMS_RELOCATION_CONFLICT);
    }
    MaviError::Internal
}

pub const FORMS_RELOCATION_CONFLICT: &str = "forms_relocation_conflict";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FormFieldKind;

    #[test]
    fn relocation_is_site_bound_and_preserves_deleted_state() {
        let site = SiteId::new();
        let form_id = Uuid::now_v7();
        let relocation = FormsRelocation {
            format: FORMS_RELOCATION_FORMAT.to_owned(),
            version: FORMS_RELOCATION_VERSION,
            source_site_id: site,
            forms: vec![FormRelocation {
                id: form_id,
                slug: "contact".to_owned(),
                name: "Contact".to_owned(),
                fields: vec![FormField {
                    key: "email".to_owned(),
                    label: "Email".to_owned(),
                    required: true,
                    kind: FormFieldKind::Email,
                    options: Vec::new(),
                }],
                open: false,
                kept_days: 365,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                deleted_at: Some(Utc::now()),
            }],
            submissions: vec![FormSubmissionRelocation {
                id: Uuid::now_v7(),
                form_id,
                answers: Map::from_iter([(
                    "email".to_owned(),
                    Value::String("a@example.test".to_owned()),
                )]),
                seen_at: None,
                created_at: Utc::now(),
                deleted_at: Some(Utc::now()),
            }],
        };
        relocation
            .validate_for_relocation(site)
            .expect("valid relocation");
        assert_eq!(relocation.record_count().expect("count"), 2);
        assert!(relocation.validate_for_relocation(SiteId::new()).is_err());
    }
}
