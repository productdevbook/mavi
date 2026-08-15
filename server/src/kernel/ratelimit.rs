//! How often one caller may do one thing.
//!
//! Counted in Postgres. A count kept in a process is a count each replica keeps
//! separately, which lets through as many multiples as there are replicas — and
//! the endpoints wearing this are the ones where that matters: sign-in, and
//! anything a visitor can post to.

use super::db::Db;
use super::error::{AppError, Result};

#[derive(Clone, Copy, Debug)]
pub struct Limit {
    pub per_window: i32,
    pub window_seconds: i64,
}

impl Limit {
    #[must_use]
    pub const fn new(per_window: i32, window_seconds: i64) -> Self {
        Self {
            per_window,
            window_seconds,
        }
    }
}

/// Attempts to spend one. `Err(RateLimited)` means it was refused.
///
/// The window is a truncation of the clock rather than a rolling count: a
/// caller at the edge of one can get up to twice the allowance across two
/// windows, and that is the trade this makes for a single row and no
/// coordination.
pub async fn spend(db: &Db, bucket: &str, limit: Limit) -> Result<()> {
    let mut tx = db.operator().await?;

    let allowed: bool = sqlx::query_scalar(
        "with window_start as (
             select to_timestamp(floor(extract(epoch from now()) / $3::double precision) * $3::double precision) as at
         )
         insert into rate_limits (bucket, window_start, count)
         select $1, at, 1 from window_start
         on conflict (bucket, window_start) do update
             set count = rate_limits.count + 1
         returning rate_limits.count <= $2",
    )
    .bind(bucket)
    .bind(limit.per_window)
    .bind(limit.window_seconds)
    .fetch_one(tx.conn())
    .await?;

    tx.commit().await?;

    if allowed {
        Ok(())
    } else {
        Err(AppError::RateLimited)
    }
}

/// Removes windows that have passed. Run from the scheduler; nothing reads a
/// window once it is over, so this is only about the table's size.
pub async fn sweep(db: &Db) -> Result<u64> {
    let mut tx = db.operator().await?;

    let removed =
        sqlx::query("delete from rate_limits where window_start < now() - interval '1 day'")
            .execute(tx.conn())
            .await?
            .rows_affected();

    tx.commit().await?;

    Ok(removed)
}
