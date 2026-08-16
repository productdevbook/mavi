//! Somebody saying a screen is broken, and the answer coming back.
//!
//! The alternative is the phone, and what was said on the phone is not beside
//! the site it was about. Anybody signed in can say something — including an
//! assistant, because telling somebody a screen is broken is not one of the
//! things a key is refused. Reading the list back is a different question: it
//! carries what `environment` gathered about whoever said it, so it asks for
//! the same grant the audit log does.

use axum::Json;
use axum::extract::{Query as HttpQuery, State as Injected};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::page::{Page, Query, older_than};
use crate::kernel::ratelimit::Limit;

/// Ten an hour. Enough for somebody having a bad morning, few enough that a
/// loop in a script does not become the machine's inbox.
const SAYING_LIMIT: Limit = Limit::new(10, 3600);

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/reports",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Audit, Access::View)),
                rate: RatePolicy::None,
            },
            ours,
        )
        .gives::<Page<Report>>(),
        Endpoint::post(
            "/api/reports",
            Guard {
                audience: Audience::User,
                needs: None,
                rate: RatePolicy::Per(SAYING_LIMIT),
            },
            say_something,
        )
        .takes::<Saying>()
        .gives::<Report>(),
    ]
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "report_kind", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ReportKind {
    Broken,
    Missing,
    Wanted,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "report_state", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ReportState {
    Said,
    Seen,
    Answered,
    Closed,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Report {
    pub id: Uuid,
    pub kind: ReportKind,
    /// What was gathered rather than asked for.
    pub environment: serde_json::Value,
    /// A picture, where one was sent.
    pub media_id: Option<Uuid>,
    pub screen: Option<String>,
    pub body: String,
    pub state: ReportState,
    pub answer: Option<String>,
    pub answered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Saying {
    /// `broken`, `missing` or `wanted`. What decides who looks at it and when.
    #[serde(default = "broken")]
    pub kind: String,
    /// Gathered rather than asked for: the browser, the window, and whatever
    /// had already gone wrong on the screen.
    #[serde(default)]
    pub environment: Option<serde_json::Value>,
    /// A picture, as a file this site already holds. Uploaded first, so that
    /// what is stored is a file rather than a megabyte of base64 in a column.
    #[serde(default)]
    pub media_id: Option<Uuid>,
    /// What they were looking at, as they were looking at it.
    #[serde(default)]
    pub screen: Option<String>,
    pub body: String,
}

/// What a report is, when nobody says: something is broken.
fn broken() -> String {
    "broken".to_owned()
}

const COLUMNS: &str = "id, kind, screen, body, state,
     environment, media_id, answer, answered_at, created_at";

/// A record of what has been said, and reading it is gated the way the audit
/// log is: not everybody with an account should see what somebody else
/// reported, or what the `environment` blob gathered about them.
async fn ours(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(page): HttpQuery<Query>,
) -> Result<Json<Page<Report>>> {
    caller.require_user()?;

    let mut conn = state.db.begin().await?;

    let rows: Vec<Report> = sqlx::query_as(&format!(
        "select {COLUMNS} from reports
          where ($1::timestamptz is null or created_at < $1)
          order by created_at desc
          limit $2"
    ))
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&page, rows, |report| {
        report.created_at.to_rfc3339()
    })))
}

/// Anybody signed in, with no grant asked for.
///
/// Deliberately: what a grant would gate here is somebody telling the people
/// who run the machine that something is broken, and a person who cannot do
/// that is a person who telephones instead.
async fn say_something(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Json(saying): Json<Saying>,
) -> Result<Audited<(StatusCode, Json<Report>)>> {
    let who = caller.require_user()?;

    if saying.body.trim().is_empty() {
        return Err(AppError::Invalid(
            crate::kernel::say::NOTHING_WAS_SAID.into(),
        ));
    }

    if !matches!(saying.kind.as_str(), "broken" | "missing" | "wanted") {
        return Err(AppError::Invalid(
            crate::kernel::say::THAT_IS_NOT_A_KIND_OF_REPORT.into(),
        ));
    }

    let mut conn = state.db.begin().await?;

    // An assistant reporting the same gap in forty sites is one thing to fix,
    // not forty, and telling them apart is what makes that visible.
    let by_a_person: (bool,) = sqlx::query_as(
        "select r.key <> 'assistant' from users u join roles r on r.id = u.role_id
          where u.id = $1",
    )
    .bind(who.user_id)
    .fetch_one(conn.conn())
    .await?;

    let said: Report = sqlx::query_as(&format!(
        "insert into reports
             (said_by, by_a_person, screen, body, kind, environment, media_id)
         values ($1, $2, $3, $4, $5::report_kind,
                 $6, $7)
         returning {COLUMNS}"
    ))
    .bind(who.user_id)
    .bind(by_a_person.0)
    .bind(saying.screen.as_deref())
    .bind(saying.body.trim())
    .bind(&saying.kind)
    .bind(
        saying
            .environment
            .clone()
            .unwrap_or_else(|| serde_json::json!({})),
    )
    .bind(saying.media_id)
    .fetch_one(conn.conn())
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "said something was wrong",
        "report",
        Some(&said.id.to_string()),
        &serde_json::json!({ "screen": saying.screen }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(said))))
}

/// The same thing, said by a tool rather than by a screen.
pub async fn said_by_a_tool(
    state: &AppState,
    caller: &Caller,
    body: &str,
    screen: Option<&str>,
) -> Result<serde_json::Value> {
    let who = caller.require_user()?;

    if body.trim().is_empty() {
        return Err(AppError::Invalid(
            crate::kernel::say::NOTHING_WAS_SAID.into(),
        ));
    }

    let mut conn = state.db.begin().await?;

    let said: Report = sqlx::query_as(&format!(
        "insert into reports (said_by, by_a_person, screen, body)
         values ($1, false, $2, $3)
         returning {COLUMNS}"
    ))
    .bind(who.user_id)
    .bind(screen)
    .bind(body.trim())
    .fetch_one(conn.conn())
    .await?;

    audit::record_raw(
        &mut conn,
        Actor::of(caller),
        "said something was wrong",
        "report",
        Some(&said.id.to_string()),
        &serde_json::json!({ "screen": screen }),
    )
    .await?;

    conn.commit().await?;

    Ok(serde_json::json!({ "id": said.id, "state": said.state }))
}
