//! Who did what, and when.
//!
//! Every change in this build writes a row here before it can answer, and until
//! now nothing could read one: a log nobody can read is a log that only exists
//! for the day somebody opens `psql`.

use axum::Json;
use axum::extract::{Query as HttpQuery, State as Injected};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::error::Result;
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::page::{Page, Query, older_than};

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/audit",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Audit, Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Entry>>(),
        Endpoint::get(
            "/api/audit/export",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Audit, Access::View)),
                rate: RatePolicy::None,
            },
            export,
        ),
    ]
}

/// One thing that happened.
///
/// What changed is here as it was written — before and after — because an audit
/// row that says only "changed" answers nothing anybody asks it.
#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Entry {
    pub id: Uuid,
    /// Who, where there was somebody. Null for the machine's own work, which
    /// says `system` beside it.
    pub actor_id: Option<Uuid>,
    pub actor_kind: String,
    /// What they are called, where the account is still here. A name is what a
    /// screen shows, and an account that has gone does not unwrite what it did.
    pub actor_name: Option<String>,
    pub action: String,
    pub subject: String,
    pub subject_id: Option<String>,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
    /// What tied it to one request, for reading the log beside a trace.
    pub request_id: String,
    pub created_at: DateTime<Utc>,
}

/// What to look at. Every one of these is a column with an index behind it, and
/// a field nothing declares is not a filter — the same rule as everywhere else.
#[derive(Debug, Deserialize)]
pub struct Filter {
    #[serde(flatten)]
    pub page: Query,
    /// One person's doing, for "what did they change".
    pub actor_id: Option<Uuid>,
    /// One kind of thing — `post`, `order`, `second_factor`.
    pub subject: Option<String>,
    /// One particular thing, for "what happened to this".
    pub subject_id: Option<String>,
    /// What was done — `wrote`, `removed`, `signed in`. Matched from the start
    /// rather than anywhere, so the index is the one that answers it.
    pub action: Option<String>,
}

async fn list(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(filter): HttpQuery<Filter>,
) -> Result<Json<Page<Entry>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Entry> = sqlx::query_as(
        "select a.id, a.actor_id, a.actor_kind, u.name as actor_name, a.action,
                a.subject, a.subject_id, a.before, a.after, a.request_id, a.created_at
           from audit_log a
           left join users u on u.id = a.actor_id
          where ($1::uuid is null or a.actor_id = $1)
            and ($2::text is null or a.subject = $2)
            and ($3::text is null or a.subject_id = $3)
            and ($4::timestamptz is null or a.created_at < $4)
            and ($6::text is null or a.action like $6 || '%')
          order by a.created_at desc
          limit $5",
    )
    .bind(filter.actor_id)
    .bind(filter.subject.as_deref())
    .bind(filter.subject_id.as_deref())
    .bind(older_than(filter.page.after.as_deref()))
    .bind(filter.page.fetch())
    .bind(filter.action.as_deref())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&filter.page, rows, |entry| {
        entry.created_at.to_rfc3339()
    })))
}

/// The whole log, as a file.
///
/// Somebody being asked what happened wants it in a spreadsheet, not in a
/// screen they scroll: this is the same rows as the listing, without the
/// paging and without what changed — the values are a site's own content and a
/// file that leaves the machine should not carry them.
async fn export(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
) -> Result<axum::response::Response> {
    use axum::response::IntoResponse as _;
    use std::fmt::Write as _;

    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Entry> = sqlx::query_as(
        "select a.id, a.actor_id, a.actor_kind, u.name as actor_name, a.action,
                a.subject, a.subject_id, null::jsonb as before, null::jsonb as after,
                a.request_id, a.created_at
           from audit_log a
           left join users u on u.id = a.actor_id
          order by a.created_at desc
          limit 10000",
    )
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    let mut out = String::from("when,who,kind,action,subject,subject_id,request_id\n");

    for entry in &rows {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{}",
            entry.created_at.to_rfc3339(),
            quoted(entry.actor_name.as_deref().unwrap_or("")),
            quoted(&entry.actor_kind),
            quoted(&entry.action),
            quoted(&entry.subject),
            quoted(entry.subject_id.as_deref().unwrap_or("")),
            quoted(&entry.request_id),
        );
    }

    let mut response = out.into_response();

    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/csv; charset=utf-8"),
    );

    response.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::HeaderValue::from_static("attachment; filename=\"record.csv\""),
    );

    Ok(response)
}

/// A field in a way a spreadsheet reads back as one field, and only as a field.
///
/// Everything is quoted rather than only what needs it: a name with a comma in
/// it is the case that breaks a file nobody checked. And anything starting the
/// way a formula starts gets an apostrophe in front, because a spreadsheet
/// reads `=...` in a cell as something to run — the name is somebody's own and
/// the file is opened on somebody else's machine.
fn quoted(value: &str) -> String {
    let a_formula = matches!(
        value.chars().next(),
        Some('=' | '+' | '-' | '@' | '\t' | '\r')
    );

    let value = if a_formula {
        format!("'{value}")
    } else {
        value.to_owned()
    };

    format!("\"{}\"", value.replace('"', "\"\""))
}
