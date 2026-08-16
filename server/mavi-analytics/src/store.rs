//! Counting, and forgetting.

use chrono::NaiveDate;
use mavi_core::error::{Error, Result};
use mavi_db::Tx;
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

/// The longest a path may be before it is not a path.
///
/// The same number the column checks. A visitor's browser sends this, so it is
/// somebody else's typing: what arrives longer than a page's address could be
/// is cut rather than refused, because a beacon that argues is a beacon that
/// loses the count it was sent for.
pub const AT_MOST: usize = 500;

/// One day of one path.
#[derive(Clone, Debug, Serialize)]
pub struct Read {
    pub on_day: NaiveDate,
    pub path: String,
    pub views: i64,
}

/// What a browser measured, gathered.
#[derive(Clone, Debug, Serialize)]
pub struct Felt {
    pub kind: String,
    pub path: String,
    /// The middle one. What somebody's site is usually like.
    pub middle: i64,
    /// The one a twentieth of readers had worse than. What the bad end
    /// actually looks like, which an average hides.
    pub bad_end: i64,
    pub how_many: i64,
}

/// Somebody read a page.
///
/// How many times, not how many people. Telling those apart means knowing
/// where a request came from, and nothing here is told that — see the
/// migration for why a wrong count of people is worse than none.
pub async fn was_read(tx: &mut Tx, on_day: NaiveDate, path: &str) -> Result<()> {
    let path: String = path.chars().take(AT_MOST).collect();

    if path.is_empty() {
        return Ok(());
    }

    sqlx::query(
        "insert into page_views (on_day, path, views) values ($1, $2, 1)
         on conflict (on_day, path) do update
            set views = page_views.views + 1, updated_at = now()",
    )
    .bind(on_day)
    .bind(&path)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(())
}

/// A browser measured something.
pub async fn felt(
    tx: &mut Tx,
    on_day: NaiveDate,
    path: &str,
    kind: &str,
    value: i32,
) -> Result<()> {
    let path: String = path.chars().take(AT_MOST).collect();

    // What a browser sends is somebody else's, so the list is checked here
    // rather than left to the column: a beacon refused by the database is a
    // five hundred where nothing is wrong with this installation.
    if path.is_empty() || !crate::WHAT_A_BROWSER_MEASURES.contains(&kind) || value < 0 {
        return Ok(());
    }

    sqlx::query("insert into vitals (id, on_day, path, kind, value) values ($1, $2, $3, $4, $5)")
        .bind(Uuid::now_v7())
        .bind(on_day)
        .bind(&path)
        .bind(kind)
        .bind(value)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    Ok(())
}

/// How many people read what, over the last so many days.
pub async fn how_many(tx: &mut Tx, over: i32) -> Result<Vec<Read>> {
    let rows = sqlx::query(
        "select on_day, path, views from page_views
          where on_day > current_date - $1 order by on_day desc, views desc",
    )
    .bind(over)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter()
        .map(|row| {
            Ok(Read {
                on_day: row.try_get("on_day").map_err(Error::internal)?,
                path: row.try_get("path").map_err(Error::internal)?,
                views: row.try_get("views").map_err(Error::internal)?,
            })
        })
        .collect()
}

/// How it felt, over the last so many days.
///
/// The middle and the bad end rather than an average, because an average hides
/// exactly the readers this is asked about.
pub async fn how_it_felt(tx: &mut Tx, over: i32) -> Result<Vec<Felt>> {
    let rows = sqlx::query(
        "select kind, path,
                percentile_disc(0.5) within group (order by value)::bigint as middle,
                percentile_disc(0.95) within group (order by value)::bigint as bad_end,
                count(*) as how_many
           from vitals
          where on_day > current_date - $1
          group by kind, path
          order by kind, how_many desc",
    )
    .bind(over)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter()
        .map(|row| {
            Ok(Felt {
                kind: row.try_get("kind").map_err(Error::internal)?,
                path: row.try_get("path").map_err(Error::internal)?,
                middle: row.try_get("middle").map_err(Error::internal)?,
                bad_end: row.try_get("bad_end").map_err(Error::internal)?,
                how_many: row.try_get("how_many").map_err(Error::internal)?,
            })
        })
        .collect()
}
