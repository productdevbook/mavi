//! What a site is, and what it holds about a person.
//!
//! Its own name and how much room it has left, what it tells something reading
//! rather than browsing, and the two things somebody may ask of a site about
//! themselves: everything it holds, and nothing.
use axum::Json;
use axum::extract::State as Injected;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::fmt::Write as _;
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::db::Tx;
use crate::kernel::error::Result;
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::ratelimit::Limit;
use crate::kernel::types::{Email, Title};

const LLMS_LIMIT: Limit = Limit::new(30, 60);

/// Every table holding something a person could ask for a copy of, and the
/// column their address is in. A domain that adds one adds a line here, and a
/// test compares this list against the retention policies.
const ABOUT_A_PERSON: &[(&str, &str)] = &[
    ("users", "email"),
    ("students", "email"),
    ("subscribers", "email"),
    ("orders", "email"),
    ("email_log", "to_email"),
];

/// One table per kind of thing a site does, for "how many rows of what kind".
/// A table added here without a matching one added to another domain's own
/// list is nobody's bug but this one's to keep current.
const ROW_KINDS: &[&str] = &[
    "posts",
    "media",
    "products",
    "orders",
    "students",
    "form_submissions",
    "subscribers",
    "users",
    "cards",
];

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/llms.txt",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(LLMS_LIMIT),
            },
            llms,
        ),
        Endpoint::get(
            "/api/site",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Settings, Access::View)),
                rate: RatePolicy::None,
            },
            settings,
        )
        .gives::<Settings>(),
        Endpoint::patch(
            "/api/site",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Settings, Access::Write)),
                rate: RatePolicy::None,
            },
            rename,
        )
        .takes::<SettingsChanges>()
        .gives::<Settings>(),
        Endpoint::get(
            "/api/site/usage",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Settings, Access::View)),
                rate: RatePolicy::None,
            },
            usage,
        )
        .gives::<Usage>(),
        Endpoint::post(
            "/api/people/export",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::People, Access::View)),
                rate: RatePolicy::None,
            },
            export,
        )
        .takes::<About>()
        .gives::<Copied>(),
        Endpoint::post(
            "/api/people/erase",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::People, Access::Delete)),
                rate: RatePolicy::None,
            },
            erase,
        )
        .takes::<About>(),
    ]
}

/// What a site's own people may see of its settings.
#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Settings {
    pub name: String,
    /// How much has been uploaded, and how much may be. Read rather than set:
    /// the ceiling is the machine's, and finding out at the point of an upload
    /// being refused is finding out too late.
    pub storage_used_bytes: i64,
    pub storage_limit_bytes: i64,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsChanges {
    pub name: Title,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct About {
    pub email: Email,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Copied {
    pub email: String,
    /// One entry per table that had something, with the rows in it.
    pub found: serde_json::Value,
}

async fn settings(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
) -> Result<Json<Settings>> {
    let mut conn = state.db.begin().await?;
    let found = read_settings(&mut conn, &caller).await?;
    conn.commit().await?;

    Ok(Json(found))
}

async fn rename(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(wanted): Json<SettingsChanges>,
) -> Result<Audited<Json<Settings>>> {
    let mut conn = state.db.begin().await?;

    // Written rather than updated: a site carried in from elsewhere may have no
    // row here, and a name that cannot be set until somebody runs an insert by
    // hand is a name that cannot be set.
    sqlx::query(
        "insert into site_settings (name) values ($1)
         on conflict ((true)) do update set name = excluded.name",
    )
    .bind(wanted.name.as_str())
    .execute(conn.conn())
    .await?;

    let after = read_settings(&mut conn, &caller).await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "renamed the site",
        "site",
        None,
        &json!({ "name": after.name }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(after)))
}

async fn read_settings(conn: &mut Tx, _caller: &Caller) -> Result<Settings> {
    let found: Option<Settings> = sqlx::query_as(
        "select s.name,
                coalesce((select sum(m.bytes) from media m
                           where m.deleted_at is null), 0)::bigint as storage_used_bytes,
                $1::bigint as storage_limit_bytes
           from site_settings s",
    )
    .bind(crate::media::MOST_BYTES_A_SITE)
    .fetch_optional(conn.conn())
    .await?;

    Ok(found.unwrap_or_else(|| Settings {
        name: String::new(),
        storage_used_bytes: 0,
        storage_limit_bytes: crate::media::MOST_BYTES_A_SITE,
    }))
}

/// What this installation is holding and what it has done — no price
/// anywhere in it. This is the site's own read of itself, for the moment
/// somebody is asking "why is my disk full" or "did that campaign actually
/// go out" rather than browsing.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Usage {
    pub storage: StorageUsage,
    /// How many rows of each kind: see `RowCount`.
    pub rows: Vec<RowCount>,
    pub mail: MailUsage,
    /// Most recent first.
    pub builds: Vec<RecentBuild>,
    pub queue: QueueUsage,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StorageUsage {
    pub used_bytes: i64,
    pub by_kind: Vec<StorageKind>,
}

#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct StorageKind {
    /// The first half of the file's own mime type: `image`, `video`,
    /// `application`, and so on — not this machine's word for it.
    pub kind: String,
    pub bytes: i64,
    pub count: i64,
}

/// Rows of one kind, counted outright.
///
/// `exact` is always `true` today. A table large enough to make that count
/// worth avoiding should fall back to what Postgres already tracks for its
/// own planner instead — the same question #60 is open about for
/// `/api/overview` — but making `pg_class.reltuples` trustworthy here turned
/// out to need more rounds than this endpoint could spend on it; see #72 for
/// what was tried and what is still unknown. `exact` stays on the shape now
/// so a caller does not have to change again when that lands.
#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct RowCount {
    pub kind: String,
    pub rows: i64,
    pub exact: bool,
}

/// What was sent, and what became of it. `attempted` is everything this site
/// tried to hand to a provider; `delivered` is what a provider confirmed
/// reaching an inbox, which is fewer things than `attempted` on a machine with
/// no delivery webhook configured, and that gap is itself an answer.
#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct MailUsage {
    pub attempted: i64,
    pub delivered: i64,
    pub bounced: i64,
    pub failed: i64,
}

/// One build. `seconds` is null for one that never finished.
#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct RecentBuild {
    pub state: String,
    pub seconds: Option<i32>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct QueueUsage {
    pub waiting: i64,
    pub running: i64,
    pub failed: i64,
    pub dead: i64,
    /// When the oldest job still waiting was queued. Null when nothing is.
    pub oldest_waiting_since: Option<DateTime<Utc>>,
}

async fn usage(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
) -> Result<Json<Usage>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    let found = read_usage(&mut conn).await?;
    conn.commit().await?;

    Ok(Json(found))
}

async fn read_usage(conn: &mut TenantConn) -> Result<Usage> {
    let by_kind: Vec<StorageKind> = sqlx::query_as(
        "select split_part(mime, '/', 1) as kind,
                sum(bytes)::bigint as bytes,
                count(*)::bigint as count
           from media
          where deleted_at is null
          group by 1
          order by 2 desc",
    )
    .fetch_all(conn.conn())
    .await?;

    let used_bytes = by_kind.iter().map(|kind| kind.bytes).sum();

    let rows = read_rows(conn).await?;

    // `email_log` is swept at two years old, so this scan is bounded by that
    // rather than by everything a site has ever sent. `delivered` comes from
    // `mail_events` rather than `email_log.state`: the state a message is
    // written with says this machine handed it to a provider, not that the
    // provider handed it to an inbox.
    let mail: MailUsage = sqlx::query_as(
        "select
            count(*) filter (where state <> 'queued') as attempted,
            (select count(distinct email_log_id) from mail_events
              where kind = 'delivered') as delivered,
            count(*) filter (where state = 'bounced') as bounced,
            count(*) filter (where state = 'failed') as failed
           from email_log",
    )
    .fetch_one(conn.conn())
    .await?;

    let builds: Vec<RecentBuild> = sqlx::query_as(
        "select state::text as state, seconds, finished_at
           from publishes
          order by created_at desc
          limit 20",
    )
    .fetch_all(conn.conn())
    .await?;

    // `done` jobs are not swept yet, so counting them here would grow with
    // everything the queue has ever finished rather than with what it is
    // holding now — `jobs_tenant_backlog_idx` exists so this stays a lookup
    // into the backlog rather than a scan of the site's whole job history.
    let queue: QueueUsage = sqlx::query_as(
        "select
            count(*) filter (where state = 'ready' and run_at <= now()) as waiting,
            count(*) filter (where state = 'running') as running,
            count(*) filter (where state = 'failed') as failed,
            count(*) filter (where state = 'dead') as dead,
            min(created_at) filter (where state = 'ready' and run_at <= now())
                as oldest_waiting_since
           from jobs
          where state <> 'done'",
    )
    .fetch_one(conn.conn())
    .await?;

    Ok(Usage {
        storage: StorageUsage {
            used_bytes,
            by_kind,
        },
        rows,
        mail,
        builds,
        queue,
    })
}

/// How many rows of each kind, one `count(*)` per table in `ROW_KINDS`.
///
/// This is exactly the cost the rest of this endpoint exists to avoid — see
/// the doc comment on `RowCount` for why, and #72 for why an estimate is not
/// standing in for it yet.
///
/// Walking `ROW_KINDS` itself, rather than a name read back from a query, is
/// what lets `{kind}` go straight into the query below: `kind` is bound by
/// this loop over a constant this crate wrote, the same rule `gather` and
/// `erase` splice a table name under, and never a value a caller sent.
async fn read_rows(conn: &mut TenantConn) -> Result<Vec<RowCount>> {
    let mut rows = Vec::with_capacity(ROW_KINDS.len());

    for &kind in ROW_KINDS {
        let (counted,): (i64,) = sqlx::query_as(&format!("select count(*)::bigint from {kind}"))
            .fetch_one(conn.conn())
            .await?;

        rows.push(RowCount {
            kind: kind.to_owned(),
            rows: counted,
            exact: true,
        });
    }

    Ok(rows)
}

/// What a site is, for something reading rather than browsing. Written from
/// what is published rather than from a file somebody has to remember.
async fn llms(Injected(state): Injected<AppState>, _caller: Caller) -> Result<Response> {
    let mut conn = state.db.begin().await?;

    let name: Option<(String,)> = sqlx::query_as("select name from site_settings ")
        .fetch_optional(conn.conn())
        .await?;

    let pages = sqlx::query(
        "select title, slug, excerpt from posts
          where state = 'published' and deleted_at is null
          order by published_at desc limit 200",
    )
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    let mut out = format!(
        "# {}\n\n",
        name.map_or_else(|| "A site".to_owned(), |(name,)| name)
    );

    out.push_str("What is published here, most recent first.\n\n");

    for page in &pages {
        let title: String = page.get("title");
        let slug: String = page.get("slug");
        let excerpt: Option<String> = page.get("excerpt");

        let _ = write!(out, "- [{title}](/{slug})");

        if let Some(excerpt) = excerpt.filter(|text| !text.trim().is_empty()) {
            let _ = write!(out, ": {}", excerpt.chars().take(200).collect::<String>());
        }

        out.push('\n');
    }

    Ok(([(CONTENT_TYPE, "text/plain; charset=utf-8")], out).into_response())
}

/// Everything this site holds about one address, in one answer.
async fn export(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<About>,
) -> Result<Audited<Json<Copied>>> {
    let mut conn = state.db.begin().await?;
    let found = gather(&mut conn, body.email.as_str()).await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "gave somebody their copy",
        "person",
        Some(body.email.as_str()),
        &json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        Json(Copied {
            email: body.email.into_string(),
            found,
        }),
    ))
}

async fn gather(conn: &mut Tx, email: &str) -> Result<serde_json::Value> {
    let mut found = serde_json::Map::new();

    for (table, column) in ABOUT_A_PERSON {
        // Both come from the constant above and never from a request: there is
        // nothing here a caller chooses.
        let rows = sqlx::query(&format!(
            "select to_jsonb(t) - 'password_hash' - 'token_hash' as row
               from {table} t where t.{column} = $1"
        ))
        .bind(email)
        .fetch_all(conn.conn())
        .await?;

        if !rows.is_empty() {
            found.insert(
                (*table).to_owned(),
                serde_json::Value::Array(
                    rows.iter()
                        .map(|row| row.get::<serde_json::Value, _>("row"))
                        .collect(),
                ),
            );
        }
    }

    Ok(serde_json::Value::Object(found))
}

/// Takes away what a site holds about one address.
///
/// What is a financial record is emptied of the person rather than deleted: an
/// order that vanishes is a bill nobody can explain, and the rule that says
/// keep it and the rule that says remove them are both true.
///
/// An address that is the site's only owner is not touched at all, here or
/// anywhere else this reaches: blanking that account the way an order is
/// blanked would leave a row that satisfies "an owner exists" in name only —
/// nobody able to sign into it, so nobody able to grant the role onward
/// either, which is worse than refusing outright. The request answers with
/// why, and a retry after another owner exists starts from everything still
/// in place.
async fn erase(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<About>,
) -> Result<Audited<Json<serde_json::Value>>> {
    let email = body.email.as_str();
    let mut conn = state.db.begin().await?;

    let holder: Option<(Uuid,)> =
        sqlx::query_as("select id from users where email = $1 and deleted_at is null")
            .bind(email)
            .fetch_optional(conn.conn())
            .await?;

    if let Some((id,)) = holder {
        crate::people::refuse_if_last_owner(&mut conn, id).await?;
    }

    let mut taken = serde_json::Map::new();

    for table in ["users", "students", "subscribers", "email_log"] {
        let column = if *table == *"email_log" {
            "to_email"
        } else {
            "email"
        };

        let gone = sqlx::query(&format!("delete from {table} where {column} = $1"))
            .bind(email)
            .execute(conn.conn())
            .await?
            .rows_affected();

        taken.insert((*table).to_owned(), json!(gone));
    }

    let emptied =
        sqlx::query("update orders set email = 'erased@example.invalid' where email = $1")
            .bind(email)
            .execute(conn.conn())
            .await?
            .rows_affected();

    taken.insert("orders".to_owned(), json!({ "emptied": emptied }));

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "erased somebody",
        "person",
        Some(email),
        &serde_json::Value::Object(taken.clone()),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        Json(serde_json::Value::Object(taken)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list a copy is gathered from and the list a retention policy is
    /// written from describe the same thing. Drifting apart is how somebody's
    /// data ends up in a table nobody thought to look in.
    #[test]
    fn everything_a_person_is_in_has_a_retention_policy() {
        use crate::kernel::retention;

        for (table, _) in ABOUT_A_PERSON {
            assert!(
                retention::policy_for(table).is_some(),
                "{table} holds somebody's own data and says nothing about keeping it"
            );
        }
    }

    #[test]
    fn nothing_gathered_is_a_secret() {
        assert!(!ABOUT_A_PERSON.iter().any(|(table, _)| *table == "sessions"));
        assert!(!ABOUT_A_PERSON.iter().any(|(table, _)| *table == "tickets"));
    }
}
