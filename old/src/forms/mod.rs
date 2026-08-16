//! What a site asks visitors, and what they send back.
//!
//! A form declares its fields and the far end checks against them: a form is a
//! public endpoint, and what a page does before posting is a courtesy rather
//! than a rule. What comes in is kept as it was sent, so changing the fields
//! later never rewrites it — and it is forgotten on the site's own schedule.
use axum::Json;
use axum::extract::{Path, Query as HttpQuery, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Auditable, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::db::Tx;
use crate::kernel::error::{AppError, Result};
use crate::kernel::events::{self, EmitsEvents};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::page::{Page, Query};
use crate::kernel::ratelimit::Limit;
use crate::kernel::say::{self, Say};
use crate::kernel::types::{Slug, Title};

/// Twenty a minute from one address. A form is the one thing on a site anybody
/// can write to, and it is where a site gets buried.
const SUBMIT_LIMIT: Limit = Limit::new(20, 60);

/// What one answer may be, and how many a submission may carry. A form with no
/// limit on it is a table somebody else decides the size of.
const MAX_ANSWER: usize = 10_000;
const MAX_ANSWERS: usize = 100;

fn needs(access: Access) -> Needs {
    Needs::new(Capability::Forms, access)
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/forms",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Form>>(),
        Endpoint::post(
            "/api/forms",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Write)),
                rate: RatePolicy::None,
            },
            create,
        )
        .takes::<NewForm>()
        .gives::<Form>(),
        Endpoint::get(
            "/api/forms/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::View)),
                rate: RatePolicy::None,
            },
            read,
        )
        .gives::<Form>(),
        Endpoint::patch(
            "/api/forms/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Write)),
                rate: RatePolicy::None,
            },
            update,
        )
        .takes::<FormChanges>()
        .gives::<Form>(),
        Endpoint::delete(
            "/api/forms/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Delete)),
                rate: RatePolicy::None,
            },
            remove,
        ),
        Endpoint::get(
            "/api/forms/{id}/submissions",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::View)),
                rate: RatePolicy::None,
            },
            submissions,
        )
        .gives::<Page<Submission>>(),
        Endpoint::post(
            "/api/sites/forms/{slug}/submissions",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(SUBMIT_LIMIT),
            },
            submit,
        )
        .takes::<Answers>()
        .gives::<Accepted>(),
        Endpoint::post(
            "/api/forms/{id}/seen",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Write)),
                rate: RatePolicy::None,
            },
            mark_seen,
        )
        .gives::<Seen>(),
        Endpoint::delete(
            "/api/forms/{id}/submissions/{submission_id}",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Delete)),
                rate: RatePolicy::None,
            },
            forget,
        ),
    ]
}

#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Form {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub fields: serde_json::Value,
    pub active: bool,
    pub retention_days: i32,
    /// How much has come in, and how much of it nobody has looked at. Counted
    /// here because a list of forms is a list somebody scans for the one with
    /// something waiting in it.
    pub submissions: i64,
    pub unseen: i64,
    pub created_at: DateTime<Utc>,
}

impl Auditable for Form {
    const SUBJECT: &'static str = "form";

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "slug": self.slug,
            "name": self.name,
            "active": self.active,
            "retention_days": self.retention_days,
        })
    }
}

#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Submission {
    pub id: Uuid,
    pub form_id: Uuid,
    pub answers: serde_json::Value,
    pub seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl EmitsEvents for Submission {
    const EVENTS: &'static [&'static str] = &["form.submitted"];

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    /// What somebody wrote is not in the event. A receiver is told that a form
    /// was filled in and where to read it, and the answers stay on the site.
    fn payload(&self) -> serde_json::Value {
        serde_json::json!({ "form_id": self.form_id, "submission_id": self.id })
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewForm {
    pub slug: Slug,
    pub name: Title,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub retention_days: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = FormField)]
pub struct Field {
    pub key: Slug,
    pub label: Title,
    pub required: bool,
    /// What may be written in it. A form that says "email" and takes anything
    /// is a form whose answers nobody can act on.
    #[serde(default)]
    pub kind: FieldKind,
    /// What a `choice` may be. Empty for every other kind.
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = FormFieldKind)]
pub enum FieldKind {
    #[default]
    Text,
    /// More than a line of it. The same rules as text, said so a screen can
    /// draw the right box.
    Long,
    Email,
    Number,
    Choice,
    Boolean,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FormChanges {
    pub name: Option<Title>,
    pub fields: Option<Vec<Field>>,
    pub active: Option<bool>,
    pub retention_days: Option<i32>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Answers {
    pub answers: serde_json::Map<String, serde_json::Value>,
}

async fn list(
    State(state): State<AppState>,
    _caller: Caller,
    permit: Permit,
    HttpQuery(query): HttpQuery<Query>,
) -> Result<Json<Page<Form>>> {
    let mut conn = state.db.begin().await?;
    let page = all(&mut conn, &permit, &query).await?;
    conn.commit().await?;

    Ok(Json(page))
}

/// Takes the permit rather than trusting its caller to have asked: a read that
/// never went past the engine cannot call this.
async fn all(conn: &mut Tx, _permit: &Permit, query: &Query) -> Result<Page<Form>> {
    let rows: Vec<Form> = sqlx::query_as(
        "select f.id, f.slug, f.name, f.fields, f.active, f.retention_days,
                (select count(*) from form_submissions s where s.form_id = f.id) as submissions,
                (select count(*) from form_submissions s
                  where s.form_id = f.id and s.seen_at is null) as unseen,
                f.created_at
           from forms f
          where f.deleted_at is null
            and ($1::timestamptz is null or f.created_at < $1)
          order by f.created_at desc, f.id desc
          limit $2",
    )
    .bind(cursor(query.after.as_deref()))
    .bind(query.fetch())
    .fetch_all(conn.conn())
    .await?;

    Ok(Page::build(query, rows, |form| {
        form.created_at.to_rfc3339()
    }))
}

fn cursor(after: Option<&str>) -> Option<DateTime<Utc>> {
    after.and_then(|value| DateTime::parse_from_rfc3339(value).ok().map(Into::into))
}

async fn create(
    State(state): State<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<NewForm>,
) -> Result<Audited<(StatusCode, Json<Form>)>> {
    let mut conn = state.db.begin().await?;

    let taken: Option<(Uuid,)> = sqlx::query_as("select id from forms where slug = $1")
        .bind(body.slug.as_str())
        .fetch_optional(conn.conn())
        .await?;

    if taken.is_some() {
        return Err(AppError::Conflict(say::FORM_ALREADY_ANSWERS_ON_NAME.into()));
    }

    let form: Form = sqlx::query_as(
        "insert into forms (slug, name, fields, retention_days)
         values ($1, $2, $3, coalesce($4, 365))
         returning id, slug, name, fields, active, retention_days,
                   0::bigint as submissions, 0::bigint as unseen, created_at",
    )
    .bind(body.slug.as_str())
    .bind(body.name.as_str())
    .bind(serde_json::to_value(&body.fields).unwrap_or_else(|_| serde_json::json!([])))
    .bind(body.retention_days)
    .fetch_one(conn.conn())
    .await?;

    let receipt =
        audit::record(&mut conn, Actor::of(&caller), "created", None, Some(&form)).await?;
    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(form))))
}

async fn read(
    State(state): State<AppState>,
    _caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Json<Form>> {
    let mut conn = state.db.begin().await?;
    let form = one(&mut conn, id).await?;
    conn.commit().await?;

    Ok(Json(form))
}

/// A form belonging to somebody else is not there, rather than refused: which
/// of those two answers comes back is how a caller finds out whether it exists.
async fn one(conn: &mut Tx, id: Uuid) -> Result<Form> {
    sqlx::query_as(
        "select f.id, f.slug, f.name, f.fields, f.active, f.retention_days,
                (select count(*) from form_submissions s where s.form_id = f.id) as submissions,
                (select count(*) from form_submissions s
                  where s.form_id = f.id and s.seen_at is null) as unseen,
                f.created_at
           from forms f where f.id = $1 and f.deleted_at is null",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?
    .ok_or(AppError::NotFound("form"))
}

async fn update(
    State(state): State<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    Json(changes): Json<FormChanges>,
) -> Result<Audited<Json<Form>>> {
    let mut conn = state.db.begin().await?;
    let before = one(&mut conn, id).await?;

    let after: Form = sqlx::query_as(
        "update forms
            set name = coalesce($2, name),
                fields = coalesce($3, fields),
                active = coalesce($4, active),
                retention_days = coalesce($5, retention_days)
          where id = $1 and deleted_at is null
         returning id, slug, name, fields, active, retention_days,
                   (select count(*) from form_submissions s where s.form_id = forms.id)
                       as submissions,
                   (select count(*) from form_submissions s
                     where s.form_id = forms.id and s.seen_at is null) as unseen,
                   created_at",
    )
    .bind(id)
    .bind(changes.name.as_ref().map(Title::as_str))
    .bind(
        changes
            .fields
            .as_ref()
            .map(|fields| serde_json::to_value(fields).unwrap_or_else(|_| serde_json::json!([]))),
    )
    .bind(changes.active)
    .bind(changes.retention_days)
    .fetch_one(conn.conn())
    .await?;

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "changed",
        Some(&before),
        Some(&after),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(after)))
}

async fn remove(
    State(state): State<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.begin().await?;
    let before = one(&mut conn, id).await?;

    sqlx::query("update forms set deleted_at = now() where id = $1")
        .bind(id)
        .execute(conn.conn())
        .await?;

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "removed",
        Some(&before),
        None,
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

async fn submissions(
    State(state): State<AppState>,
    _caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    HttpQuery(query): HttpQuery<Query>,
) -> Result<Json<Page<Submission>>> {
    let mut conn = state.db.begin().await?;
    one(&mut conn, id).await?;

    let rows: Vec<Submission> = sqlx::query_as(
        "select id, form_id, answers, seen_at, created_at
           from form_submissions
          where form_id = $1
            and deleted_at is null
            and ($2::timestamptz is null or created_at < $2)
          order by created_at desc, id desc
          limit $3",
    )
    .bind(id)
    .bind(cursor(query.after.as_deref()))
    .bind(query.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&query, rows, |submission| {
        submission.created_at.to_rfc3339()
    })))
}

/// The one endpoint here a visitor reaches. Everything it is given is bounded
/// before it is stored, and the form is found by the name on the address rather
/// than by an id the caller supplies.
/// How many were waiting.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Seen {
    pub seen: i64,
}

/// Everything waiting on one form, marked as read.
///
/// One call rather than one per row: what a person does is open the list, read
/// it, and be done with it — and a request per submission is a screen that
/// takes a second to close.
async fn mark_seen(
    State(state): State<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<Json<Seen>>> {
    let mut conn = state.db.begin().await?;

    let seen = sqlx::query(
        "update form_submissions set seen_at = now()
          where form_id = $1 and seen_at is null",
    )
    .bind(id)
    .execute(conn.conn())
    .await?
    .rows_affected();

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "read what came in",
        "form",
        Some(&id.to_string()),
        &serde_json::json!({ "seen": seen }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        Json(Seen {
            seen: i64::try_from(seen).unwrap_or(i64::MAX),
        }),
    ))
}

/// One submission, gone.
///
/// Hard rather than soft: what somebody sent is theirs, and "delete it" from a
/// person who wrote in is not answered by hiding it. What it said is not
/// written into the record either — the whole point of taking it away.
async fn forget(
    State(state): State<AppState>,
    caller: Caller,
    _permit: Permit,
    Path((id, submission_id)): Path<(Uuid, Uuid)>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.begin().await?;

    let gone = sqlx::query("delete from form_submissions where id = $1 and form_id = $2")
        .bind(submission_id)
        .bind(id)
        .execute(conn.conn())
        .await?
        .rows_affected();

    if gone == 0 {
        return Err(AppError::NotFound("submission"));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "took a submission away",
        "form_submission",
        Some(&submission_id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

/// Whether what somebody sent is what the form asked for.
///
/// Checked here rather than in a browser: a form is a public endpoint, and
/// what a page does before posting is a courtesy rather than a rule. A form
/// that declares nothing takes anything, which is what a site with a hand-made
/// page in front of it is relying on.
fn fits(declared: &[Field], answers: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    if declared.is_empty() {
        return Ok(());
    }

    for field in declared {
        let given = answers.get(field.key.as_str());

        let empty = match given {
            None | Some(serde_json::Value::Null) => true,
            Some(serde_json::Value::String(text)) => text.trim().is_empty(),
            Some(_) => false,
        };

        if field.required && empty {
            return Err(AppError::Invalid(
                Say::of(say::THAT_FORM_WANTS_THAT_FIELD).naming("field", field.key.as_str()),
            ));
        }

        let Some(value) = given.filter(|_| !empty) else {
            continue;
        };

        let holds = match field.kind {
            FieldKind::Text | FieldKind::Long => value.is_string(),
            FieldKind::Email => value
                .as_str()
                .is_some_and(|written| crate::kernel::types::Email::parse(written).is_ok()),
            FieldKind::Number => value.is_number(),
            FieldKind::Boolean => value.is_boolean(),
            FieldKind::Choice => value
                .as_str()
                .is_some_and(|written| field.options.iter().any(|one| one == written)),
        };

        if !holds {
            return Err(AppError::Invalid(
                Say::of(say::THAT_IS_NOT_WHAT_THAT_FIELD_HOLDS).naming("field", field.key.as_str()),
            ));
        }
    }

    // A key nothing declared is somebody posting at the form rather than
    // filling it in, and keeping it would put whatever they sent in front of
    // whoever reads the submissions.
    if let Some(unknown) = answers
        .keys()
        .find(|key| !declared.iter().any(|field| field.key.as_str() == *key))
    {
        return Err(AppError::Invalid(
            Say::of(say::THAT_FORM_HAS_NO_SUCH_FIELD).naming("field", unknown),
        ));
    }

    Ok(())
}

async fn submit(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
    Json(body): Json<Answers>,
) -> Result<Audited<(StatusCode, Json<Accepted>)>> {
    let slug = Slug::parse(&slug)?;

    if body.answers.len() > MAX_ANSWERS {
        return Err(AppError::Invalid(say::TOO_MANY_ANSWERS.into()));
    }

    for value in body.answers.values() {
        let too_long = match value {
            serde_json::Value::String(text) => text.len() > MAX_ANSWER,
            other => other.to_string().len() > MAX_ANSWER,
        };

        if too_long {
            return Err(AppError::Invalid(say::ANSWER_TOO_LONG.into()));
        }
    }

    let mut conn = state.db.begin().await?;

    let form: Option<(Uuid, serde_json::Value)> = sqlx::query_as(
        "select id, fields from forms where slug = $1 and active and deleted_at is null",
    )
    .bind(slug.as_str())
    .fetch_optional(conn.conn())
    .await?;

    let Some((form_id, declared)) = form else {
        return Err(AppError::NotFound("form"));
    };

    let declared: Vec<Field> = serde_json::from_value(declared).unwrap_or_default();

    fits(&declared, &body.answers)?;

    let submission: Submission = sqlx::query_as(
        "insert into form_submissions (form_id, answers, from_ip, user_agent)
         values ($1, $2, $3::text::inet, $4)
         returning id, form_id, answers, seen_at, created_at",
    )
    .bind(form_id)
    .bind(serde_json::Value::Object(body.answers))
    .bind(caller.ip.as_deref())
    .bind(None::<String>)
    .fetch_one(conn.conn())
    .await?;

    events::emit(&state, &mut conn, "form.submitted", &submission).await?;

    // A visitor has no account, so what is recorded is the submission itself.
    let receipt = audit::record_raw(
        &mut conn,
        Actor::system(caller.request_id),
        "submitted",
        "form_submission",
        Some(&submission.id.to_string()),
        &serde_json::json!({ "form_id": form_id }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        (StatusCode::CREATED, Json(Accepted { id: submission.id })),
    ))
}

/// What a visitor is told: that it arrived. Nothing about the site, nothing
/// about what else is on it.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Accepted {
    pub id: Uuid,
}

impl sqlx::FromRow<'_, sqlx::postgres::PgRow> for Form {
    fn from_row(row: &sqlx::postgres::PgRow) -> sqlx::Result<Self> {
        use sqlx::Row as _;

        Ok(Self {
            id: row.try_get("id")?,
            slug: row.try_get("slug")?,
            name: row.try_get("name")?,
            fields: row.try_get("fields")?,
            active: row.try_get("active")?,
            retention_days: row.try_get("retention_days")?,
            submissions: row.try_get("submissions")?,
            unseen: row.try_get("unseen")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

impl sqlx::FromRow<'_, sqlx::postgres::PgRow> for Submission {
    fn from_row(row: &sqlx::postgres::PgRow) -> sqlx::Result<Self> {
        use sqlx::Row as _;

        Ok(Self {
            id: row.try_get("id")?,
            form_id: row.try_get("form_id")?,
            answers: row.try_get("answers")?,
            seen_at: row.try_get("seen_at")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

/// Takes away what people sent for longer than the site said to keep it. Runs
/// per site, because how long is a thing each site decides.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Sweep;

impl crate::kernel::queue::Task for Sweep {
    const KIND: &'static str = "forms.sweep";
}

pub async fn sweep(state: &AppState) -> Result<u64> {
    let mut conn = state.db.begin().await?;

    let taken = sqlx::query(
        "delete from form_submissions s
          using forms f
          where s.form_id = f.id
            and s.created_at < now() - make_interval(days => f.retention_days)",
    )
    .execute(conn.conn())
    .await?
    .rows_affected();

    conn.commit().await?;

    Ok(taken)
}
