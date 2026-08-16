//! How many people read the site, and how it felt to them.
//!
//! Counted where the pages are served and kept without anything that says who
//! anybody was: a day's salt turns an address into a mark, the mark answers
//! "has this day seen them before", and the mark goes when the day is over.
//! What is left is a number, a path, and what the browser measured.
use axum::Json;
use axum::extract::State as Injected;
use axum::http::StatusCode;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::error::Result;
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::queue::Task;
use crate::kernel::ratelimit::Limit;

/// A page is read once by a person and told about once. Sixty a minute from one
/// address is a browser doing something odd, not somebody reading.
const BEACON_LIMIT: Limit = Limit::new(60, 60);

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::post(
            "/api/sites/beacon",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(BEACON_LIMIT),
            },
            beacon,
        )
        .takes::<Beacon>(),
        Endpoint::get(
            "/api/analytics",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Settings, Access::View)),
                rate: RatePolicy::None,
            },
            summary,
        )
        .gives::<Summary>(),
        Endpoint::get(
            "/api/analytics/vitals",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Settings, Access::View)),
                rate: RatePolicy::None,
            },
            vitals,
        )
        .gives::<Vitals>(),
        Endpoint::get(
            "/api/overview",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Settings, Access::View)),
                rate: RatePolicy::None,
            },
            overview,
        )
        .gives::<Overview>(),
    ]
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Beacon {
    pub path: String,
    /// What the browser measured, where it measured anything. None of it is
    /// required: a page that only says it was read is a page that was read.
    #[serde(default)]
    pub lcp: Option<i32>,
    #[serde(default)]
    pub inp: Option<i32>,
    #[serde(default)]
    pub cls: Option<i32>,
    #[serde(default)]
    pub ttfb: Option<i32>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Summary {
    pub days: Vec<Day>,
    /// The pages people actually read, most read first. Where a site finds out
    /// that the page it worked on is not the one anybody opens.
    pub pages: Vec<Read>,
}

/// One page, and how much it was read.
#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
#[schema(as = PageRead)]
pub struct Read {
    pub path: String,
    pub views: i64,
    pub visitors: i64,
}

/// What a site adds up to.
///
/// One question rather than a screen making eleven: the first thing anybody
/// opens is this, and eleven requests is eleven chances to draw half of it.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Overview {
    pub days: i32,
    pub totals: Totals,
    /// Day by day over the window asked for, oldest first.
    pub written: Vec<Count>,
    pub arrived: Vec<Count>,
    pub visitors: Vec<Day>,
    pub mail: Mail,
}

#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Totals {
    pub posts: i64,
    pub media: i64,
    pub media_bytes: i64,
    pub forms: i64,
    pub submissions: i64,
    /// What has come in through a form and nobody has read.
    pub unseen: i64,
    pub subscribers: i64,
    pub students: i64,
    pub flows: i64,
    pub flows_on: i64,
    pub orders: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Count {
    pub on_day: NaiveDate,
    pub count: i64,
}

/// What was sent, and what became of it.
#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Mail {
    pub sent: i64,
    pub failed: i64,
    pub bounced: i64,
    pub complained: i64,
}

/// How the site felt to whoever was on it.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Vitals {
    pub days: i32,
    pub scores: Vec<Score>,
    /// The pages doing worst on the one that matters most for the feeling of a
    /// page loading. A screen that says a site is slow and not which page is a
    /// screen nobody can act on.
    pub worst: Vec<Slow>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Score {
    pub kind: String,
    /// The seventy-fifth: what three in four people got or better, which is
    /// what these are judged by everywhere else.
    pub p75: i32,
    pub samples: i64,
    /// `good`, `could_be_better` or `poor`, by the thresholds published for
    /// each measurement rather than by anything decided here.
    pub verdict: String,
}

/// One page, at the seventy-fifth of each measurement.
///
/// All four rather than the one it is ordered by: somebody looking at the
/// slowest page wants to know which part of it is slow, and a second request
/// per page to find out is a screen that makes twenty requests.
#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Slow {
    pub path: String,
    pub lcp: Option<i32>,
    pub inp: Option<i32>,
    pub cls: Option<i32>,
    pub ttfb: Option<i32>,
    pub samples: i64,
}

/// What counts as good, and what counts as bad, for each measurement.
///
/// Milliseconds except `cls`, which is a ratio kept in hundredths — so 0.1 and
/// 0.25 are 10 and 25.
const THRESHOLDS: [(&str, i32, i32); 4] = [
    ("lcp", 2_500, 4_000),
    ("inp", 200, 500),
    ("cls", 10, 25),
    ("ttfb", 800, 1_800),
];

#[derive(Debug, Deserialize)]
pub struct Over {
    /// How far back to look. A month unless asked otherwise, and never further
    /// than what is kept.
    pub days: Option<i32>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Day {
    pub on_day: NaiveDate,
    pub views: i64,
    pub visitors: i64,
}

/// What a page tells the site about itself.
///
/// No address is stored. Today's salt hashes it into a mark, the mark says
/// whether this day has seen them before, and the mark goes when the day is
/// over — so what is kept cannot be turned back into who somebody was.
async fn beacon(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Json(body): Json<Beacon>,
) -> Result<Audited<StatusCode>> {
    let today = state.clock.now().date_naive();
    let path: String = body.path.chars().take(500).collect();

    let mut conn = state.db.begin().await?;

    let mark = mark_of(today, caller.ip.as_deref());

    let first_today = sqlx::query(
        "insert into visitor_marks (on_day, mark) values ($1, $2)
         on conflict do nothing",
    )
    .bind(today)
    .bind(&mark[..])
    .execute(conn.conn())
    .await?
    .rows_affected();

    sqlx::query(
        "insert into page_views (on_day, path, views, visitors)
         values ($1, $2, 1, $3)
         on conflict (on_day, path) do update
             set views = page_views.views + 1,
                 visitors = page_views.visitors + $3",
    )
    .bind(today)
    .bind(&path)
    .bind(i64::from(first_today > 0))
    .execute(conn.conn())
    .await?;

    for (kind, value) in [
        ("lcp", body.lcp),
        ("inp", body.inp),
        ("cls", body.cls),
        ("ttfb", body.ttfb),
    ] {
        if let Some(value) = value.filter(|value| *value >= 0 && *value < 600_000) {
            sqlx::query(
                "insert into vitals (on_day, path, kind, value)
                 values ($1, $2, $3::vital_kind, $4)",
            )
            .bind(today)
            .bind(&path)
            .bind(kind)
            .bind(value)
            .execute(conn.conn())
            .await?;
        }
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::system(caller.request_id),
        "counted",
        "page_view",
        Some(&path),
        &json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

/// A day's salt is the site and the day. Two sites do not make the same mark
/// for one person, and neither does the same site tomorrow.
fn mark_of(day: NaiveDate, ip: Option<&str>) -> [u8; 32] {
    Sha256::digest(format!("{day}:{}", ip.unwrap_or("nowhere")).as_bytes()).into()
}

async fn summary(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
    axum::extract::Query(over): axum::extract::Query<Over>,
) -> Result<Json<Summary>> {
    let days = over.days.unwrap_or(30).clamp(1, 365);
    let mut conn = state.db.begin().await?;

    let rows = sqlx::query(
        "select on_day, sum(views)::bigint as views, sum(visitors)::bigint as visitors
           from page_views
          where on_day > current_date - $1::int
          group by on_day
          order by on_day desc",
    )
    .bind(days)
    .fetch_all(conn.conn())
    .await?;

    let pages: Vec<Read> = sqlx::query_as(
        "select path, sum(views)::bigint as views, sum(visitors)::bigint as visitors
           from page_views
          where on_day > current_date - $1::int
          group by path
          order by 2 desc
          limit 20",
    )
    .bind(days)
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Summary {
        pages,
        days: rows
            .iter()
            .map(|row| Day {
                on_day: row.get("on_day"),
                views: row.get("views"),
                visitors: row.get("visitors"),
            })
            .collect(),
    }))
}

async fn vitals(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    axum::extract::Query(over): axum::extract::Query<Over>,
) -> Result<Json<Vitals>> {
    Ok(Json(read_vitals(&state, &caller, over).await?))
}

async fn read_vitals(state: &AppState, _caller: &Caller, over: Over) -> Result<Vitals> {
    let days = over.days.unwrap_or(30).clamp(1, 90);
    let mut conn = state.db.begin().await?;

    let measured = sqlx::query(
        "select kind::text as kind,
                percentile_cont(0.75) within group (order by value)::int as p75,
                count(*)::bigint as samples
           from vitals
          where on_day > current_date - $1::int
          group by kind",
    )
    .bind(days)
    .fetch_all(conn.conn())
    .await?;

    let worst: Vec<Slow> = sqlx::query_as(
        "select path,
                percentile_cont(0.75) within group (order by value)
                    filter (where kind = 'lcp')::int as lcp,
                percentile_cont(0.75) within group (order by value)
                    filter (where kind = 'inp')::int as inp,
                percentile_cont(0.75) within group (order by value)
                    filter (where kind = 'cls')::int as cls,
                percentile_cont(0.75) within group (order by value)
                    filter (where kind = 'ttfb')::int as ttfb,
                count(*)::bigint as samples
           from vitals
          where on_day > current_date - $1::int
          group by path
         having count(*) filter (where kind = 'lcp') >= 5
          order by percentile_cont(0.75) within group (order by value)
                       filter (where kind = 'lcp') desc
          limit 10",
    )
    .bind(days)
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    let scores = measured
        .iter()
        .map(|row| {
            let kind: String = row.get("kind");
            let p75: i32 = row.get("p75");

            Score {
                verdict: verdict_on(&kind, p75).to_owned(),
                kind,
                p75,
                samples: row.get("samples"),
            }
        })
        .collect();

    Ok(Vitals {
        days,
        scores,
        worst,
    })
}

/// The same reading an assistant asks for, through a tool rather than a screen.
pub async fn scores_through_a_tool(
    state: &AppState,
    caller: &Caller,
    arguments: &serde_json::Value,
) -> Result<serde_json::Value> {
    let days = arguments
        .get("days")
        .and_then(serde_json::Value::as_i64)
        .and_then(|days| i32::try_from(days).ok());

    let read = read_vitals(state, caller, Over { days }).await?;

    Ok(serde_json::json!(read))
}

async fn overview(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
    axum::extract::Query(over): axum::extract::Query<Over>,
) -> Result<Json<Overview>> {
    let days = over.days.unwrap_or(30).clamp(1, 365);
    let mut conn = state.db.begin().await?;

    let totals: Totals = sqlx::query_as(
        "select
            (select count(*) from posts where deleted_at is null) as posts,
            (select count(*) from media where deleted_at is null) as media,
            (select coalesce(sum(bytes), 0)::bigint from media
              where deleted_at is null) as media_bytes,
            (select count(*) from forms where deleted_at is null) as forms,
            (select count(*) from form_submissions) as submissions,
            (select count(*) from form_submissions where seen_at is null) as unseen,
            (select count(*) from subscribers where state = 'subscribed') as subscribers,
            (select count(*) from students where deleted_at is null) as students,
            (select count(*) from flows) as flows,
            (select count(*) from flows where active) as flows_on,
            (select count(*) from orders) as orders",
    )
    .fetch_one(conn.conn())
    .await?;

    let written: Vec<Count> = sqlx::query_as(
        "select created_at::date as on_day, count(*) as count
           from posts
          where deleted_at is null and created_at > current_date - $1::int
          group by 1 order by 1",
    )
    .bind(days)
    .fetch_all(conn.conn())
    .await?;

    let arrived: Vec<Count> = sqlx::query_as(
        "select created_at::date as on_day, count(*) as count
           from form_submissions
          where created_at > current_date - $1::int
          group by 1 order by 1",
    )
    .bind(days)
    .fetch_all(conn.conn())
    .await?;

    let visitors = sqlx::query(
        "select on_day, sum(views)::bigint as views, sum(visitors)::bigint as visitors
           from page_views
          where on_day > current_date - $1::int
          group by on_day order by on_day",
    )
    .bind(days)
    .fetch_all(conn.conn())
    .await?;

    let mail: Mail = sqlx::query_as(
        "select
            count(*) filter (where state = 'sent') as sent,
            count(*) filter (where state = 'failed') as failed,
            count(*) filter (where state = 'bounced') as bounced,
            count(*) filter (where state = 'complained') as complained
           from email_log
          where created_at > current_date - $1::int",
    )
    .bind(days)
    .fetch_one(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Overview {
        days,
        totals,
        written,
        arrived,
        visitors: visitors
            .iter()
            .map(|row| Day {
                on_day: row.get("on_day"),
                views: row.get("views"),
                visitors: row.get("visitors"),
            })
            .collect(),
        mail,
    }))
}

fn verdict_on(kind: &str, p75: i32) -> &'static str {
    let Some(&(_, good, poor)) = THRESHOLDS.iter().find(|(name, _, _)| *name == kind) else {
        return "unknown";
    };

    if p75 <= good {
        "good"
    } else if p75 <= poor {
        "could_be_better"
    } else {
        "poor"
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SweepMarks;

impl Task for SweepMarks {
    const KIND: &'static str = "analytics.sweep-marks";
}

#[must_use]
pub fn kinds() -> Vec<String> {
    vec![SweepMarks::KIND.to_owned()]
}

/// Yesterday's marks, which have done what they were for. The counts stay.
pub async fn sweep_marks(state: &AppState) -> Result<u64> {
    let mut conn = state.db.begin().await?;

    let taken = sqlx::query("delete from visitor_marks where on_day < current_date - 2")
        .execute(conn.conn())
        .await?
        .rows_affected();

    sqlx::query("delete from vitals where on_day < current_date - 90")
        .execute(conn.conn())
        .await?;

    conn.commit().await?;

    Ok(taken)
}
