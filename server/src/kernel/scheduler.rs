//! What runs on its own, and how often.
//!
//! A lock in the database rather than a leader election: whichever worker gets
//! it does the minute's work, and the rest carry on with the queue. A machine
//! running two workers does not run the sweep twice.
use chrono::Utc;
use sqlx::Row;

use super::error::Result;
use super::http::AppState;
use super::metrics;
use super::queue::{self, Task};

/// One lock for the whole cluster. Two replicas both deciding it is time to
/// reconcile payments is money moving twice, so the decision is taken by
/// whichever process holds this and skipped by the rest.
const SCHEDULER_LOCK: i64 = 0x6d_61_76_69;

/// How often something wants to run.
///
/// Not a cron expression: what anything here needs is "about this often", and
/// a schedule nobody can read is a schedule nobody notices is wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Every {
    /// Work that is late the moment it is due: a post scheduled for noon, a
    /// stock hold that has lapsed.
    Minute,
    Hour,
    Day,
}

impl Every {
    #[must_use]
    pub const fn minutes(self) -> i64 {
        match self {
            Every::Minute => 1,
            Every::Hour => 60,
            Every::Day => 60 * 24,
        }
    }
}

/// Puts a day's work in the queue, once, for every site that is live.
pub async fn daily<T: Task + Default>(state: &AppState) -> Result<Option<usize>> {
    every::<T>(state, Every::Day).await
}

/// The same, as often as something asks for. Returns how many were scheduled,
/// or nothing if another process is doing it.
pub async fn every<T: Task + Default>(state: &AppState, how_often: Every) -> Result<Option<usize>> {
    let Some((conn, sites)) = due_sites(state).await? else {
        return Ok(None);
    };

    let mut scheduled = 0;
    for site in &sites {
        let tenant = super::TenantId(site.get("id"));
        let mut work = state.db.tenant(tenant).await?;
        if already_scheduled(&mut work, T::KIND, how_often).await? {
            continue;
        }
        queue::enqueue(&mut work, &T::default(), None).await?;
        work.commit().await?;
        scheduled += 1;
    }

    conn.commit().await?;
    metrics::job_finished(T::KIND, Utc::now());
    Ok(Some(scheduled))
}

/// The same, for a kind known only by name — what an outside crate's own
/// schedule has, having no [`Task`] type this crate could name to enqueue
/// with.
pub async fn every_named(
    state: &AppState,
    kind: &'static str,
    how_often: Every,
) -> Result<Option<usize>> {
    let Some((conn, sites)) = due_sites(state).await? else {
        return Ok(None);
    };

    let mut scheduled = 0;
    for site in &sites {
        let tenant = super::TenantId(site.get("id"));
        let mut work = state.db.tenant(tenant).await?;
        if already_scheduled(&mut work, kind, how_often).await? {
            continue;
        }
        queue::enqueue_kind(&mut work, kind, None).await?;
        work.commit().await?;
        scheduled += 1;
    }

    conn.commit().await?;
    metrics::job_finished(kind, Utc::now());
    Ok(Some(scheduled))
}

/// Takes the scheduler lock and lists the sites live right now, or `None` if
/// another process already holds the lock this tick.
async fn due_sites(
    state: &AppState,
) -> Result<Option<(super::db::OperatorConn, Vec<sqlx::postgres::PgRow>)>> {
    let mut conn = state.db.operator().await?;

    let held: bool = sqlx::query_scalar("select pg_try_advisory_xact_lock($1)")
        .bind(SCHEDULER_LOCK)
        .fetch_one(conn.conn())
        .await?;

    if !held {
        return Ok(None);
    }

    let sites = sqlx::query("select id from tenants where state = 'live'")
        .fetch_all(conn.conn())
        .await?;

    Ok(Some((conn, sites)))
}

/// Nothing already waiting, and nothing finished so recently that it is not
/// due yet: a scheduler that looks every minute must not leave a day's work
/// in the queue sixty times over.
async fn already_scheduled(
    work: &mut super::db::TenantConn,
    kind: &str,
    how_often: Every,
) -> Result<bool> {
    let already: Option<(i64,)> = sqlx::query_as(
        "select count(*) from jobs
          where kind = $1
            and (state in ('ready', 'running')
                 or (finished_at is not null
                     and finished_at > now() - make_interval(mins => $2)))",
    )
    .bind(kind)
    .bind(i32::try_from(how_often.minutes()).unwrap_or(i32::MAX))
    .fetch_optional(work.conn())
    .await?;

    Ok(already.is_some_and(|(count,)| count > 0))
}
