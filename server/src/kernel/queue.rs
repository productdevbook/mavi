//! Work that happens after the answer.
//!
//! One table, claimed with `for update skip locked` and a lease: two workers
//! reaching for the same row is one of them getting it, and a worker that dies
//! holding one loses it when the lease runs out rather than keeping it for
//! ever.
//!
//! What has been tried too often goes to the dead letter rather than round
//! again, because a job that fails the same way forty times is a thing to look
//! at rather than a thing to retry.
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::Row;
use uuid::Uuid;

use super::db::{Db, TenantConn};
use super::error::Result;
use super::tenant::TenantId;

/// How long a claim is good for before another worker may take the job. Work
/// that runs longer than this calls [`extend`] while it runs.
pub const LEASE_SECONDS: i64 = 300;

fn lease_until() -> DateTime<Utc> {
    Utc::now() + Duration::seconds(LEASE_SECONDS)
}

#[derive(Clone, Debug)]
pub struct Job {
    pub id: Uuid,
    pub tenant_id: Option<TenantId>,
    pub kind: String,
    pub payload: serde_json::Value,
    pub attempts: i32,
    pub max_attempts: i32,
}

/// A kind of work, with the shape of what it needs to run. Nothing goes into
/// the queue as loose JSON under a name somebody typed.
pub trait Task: Serialize + DeserializeOwned + Send + Sync + 'static {
    const KIND: &'static str;
}

impl Job {
    /// A payload that no longer fits its type is a bug rather than a retry: the
    /// shape changed under work already queued.
    pub fn task<T: Task>(&self) -> Result<T> {
        serde_json::from_value(self.payload.clone())
            .map_err(|_| super::error::AppError::Bug("a queued job no longer fits its type"))
    }
}

/// Schedules work in the transaction that is making the change it belongs to.
pub async fn enqueue<T: Task>(
    tx: &mut TenantConn,
    task: &T,
    run_at: Option<DateTime<Utc>>,
) -> Result<Uuid> {
    enqueue_raw(tx, T::KIND, task, run_at).await
}

/// Schedules work of a kind known only as a string — what an outside crate's
/// own schedule has, having no [`Task`] type this crate could name.
pub async fn enqueue_kind(
    tx: &mut TenantConn,
    kind: &str,
    run_at: Option<DateTime<Utc>>,
) -> Result<Uuid> {
    enqueue_raw(tx, kind, &serde_json::json!({}), run_at).await
}

async fn enqueue_raw(
    tx: &mut TenantConn,
    kind: &str,
    payload: &impl Serialize,
    run_at: Option<DateTime<Utc>>,
) -> Result<Uuid> {
    let tenant = tx.tenant();
    let payload = serde_json::to_value(payload).unwrap_or(serde_json::Value::Null);

    let row = sqlx::query(
        "insert into jobs (tenant_id, kind, payload, run_at)
         values ($1, $2, $3, coalesce($4, now()))
         returning id",
    )
    .bind(tenant.0)
    .bind(kind)
    .bind(payload)
    .bind(run_at)
    .fetch_one(tx.conn())
    .await?;

    Ok(row.get("id"))
}

/// Takes one job of any tenant's, skipping whatever another worker is holding.
///
/// `SKIP LOCKED` is what lets a second replica run: two workers reaching for
/// the same row do not queue behind each other, they take different rows.
pub async fn claim(db: &Db, worker: &str, kinds: &[String]) -> Result<Option<Job>> {
    claim_within(db, worker, kinds, None).await
}

/// The same, for a worker told to take one site's work only — and for a test,
/// which shares the queue with every other test in the run.
pub async fn claim_within(
    db: &Db,
    worker: &str,
    kinds: &[String],
    tenant: Option<TenantId>,
) -> Result<Option<Job>> {
    let mut tx = db.operator().await?;

    sqlx::query("select set_config('app.worker', 'on', true)")
        .execute(tx.conn())
        .await?;

    let claimed_until = lease_until();

    let row = sqlx::query(
        "update jobs
            set state = 'running',
                claimed_until = $1,
                claimed_by = $2,
                attempts = attempts + 1
          where id = (
                select id from jobs
                 where kind = any($3)
                   and ($4::uuid is null or tenant_id = $4)
                   and (
                        (state = 'ready' and run_at <= now())
                     or (state = 'running' and claimed_until < now())
                   )
                 order by run_at
                 for update skip locked
                 limit 1
          )
         returning id, tenant_id, kind, payload, attempts, max_attempts",
    )
    .bind(claimed_until)
    .bind(worker)
    .bind(kinds)
    .bind(tenant.map(|tenant| tenant.0))
    .fetch_optional(tx.conn())
    .await?;

    tx.commit().await?;

    Ok(row.map(|row| Job {
        id: row.get("id"),
        tenant_id: row.get::<Option<Uuid>, _>("tenant_id").map(TenantId),
        kind: row.get("kind"),
        payload: row.get("payload"),
        attempts: row.get("attempts"),
        max_attempts: row.get("max_attempts"),
    }))
}

/// Says the work is still running. A job whose lease lapses is taken by
/// somebody else, so anything long-running says so before that happens.
pub async fn extend(db: &Db, job: Uuid, worker: &str) -> Result<bool> {
    let done = worker_query(
        db,
        "update jobs
            set claimed_until = $1
          where id = $2 and claimed_by = $3 and state = 'running'",
        |q| q.bind(lease_until()).bind(job).bind(worker),
    )
    .await?;

    Ok(done > 0)
}

pub async fn complete(db: &Db, job: Uuid) -> Result<()> {
    worker_query(
        db,
        "update jobs
            set state = 'done', claimed_until = null, finished_at = now()
          where id = $1",
        |q| q.bind(job),
    )
    .await?;

    Ok(())
}

/// Puts the job back, or gives up on it once it has been tried enough times.
/// A dead job stays in the table: what failed and why is the thing somebody
/// asks about, and deleting it answers nothing.
pub async fn fail(db: &Db, job: Uuid, error: &str, retry_in: Duration) -> Result<()> {
    worker_query(
        db,
        "update jobs
            set state = case when attempts >= max_attempts then 'dead'::job_state
                             else 'ready'::job_state end,
                claimed_until = null,
                claimed_by = null,
                last_error = $2,
                run_at = now() + make_interval(secs => $3),
                finished_at = case when attempts >= max_attempts then now() end
          where id = $1",
        // `make_interval` takes seconds as a float; going through i32 keeps the
        // conversion exact rather than merely close.
        |q| {
            let secs = i32::try_from(retry_in.num_seconds()).unwrap_or(i32::MAX);
            q.bind(job).bind(error).bind(f64::from(secs))
        },
    )
    .await?;

    Ok(())
}

/// Doubling, from a second: a failure that is somebody else being down is not
/// helped by asking again immediately.
#[must_use]
pub fn backoff(attempts: i32) -> Duration {
    let exponent = u32::try_from(attempts).unwrap_or(0).min(12);
    let seconds = 2_i64.saturating_pow(exponent);
    Duration::seconds(seconds.min(3600))
}

async fn worker_query<'q, F>(db: &Db, sql: &'q str, bind: F) -> Result<u64>
where
    F: FnOnce(
        sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    ) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
{
    let mut tx = db.operator().await?;

    sqlx::query("select set_config('app.worker', 'on', true)")
        .execute(tx.conn())
        .await?;

    let done = bind(sqlx::query(sql))
        .execute(tx.conn())
        .await?
        .rows_affected();

    tx.commit().await?;

    Ok(done)
}
