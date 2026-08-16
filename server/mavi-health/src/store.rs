//! The handful of things worth asking, in one query each.
//!
//! Not everything that could be measured. A health screen with forty rows on
//! it is one nobody reads, and what is here is what has actually gone wrong:
//! a site with nothing on it, a design that would not build, and work the
//! queue has given up on.
//!
//! Two of the crate this replaces are gone rather than ported, and each for a
//! reason rather than for want of time. It asked whether a site's own mail
//! server was working — mail is a port now, so what sends it is the host's and
//! this software has nothing to check. And it asked whether a site's addresses
//! answered — there are no addresses here: a request arrives at this
//! installation because this installation is what it reached.

use mavi_core::error::{Error, Result};
use mavi_db::Tx;
use serde::Serialize;

/// One thing that is either well or not.
///
/// `what` is a key rather than a sentence, so a panel words it in somebody's
/// own language — the same rule every refusal follows.
#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub what: &'static str,
    pub well: bool,
    /// What was found, where a number is what makes it worth reading.
    pub detail: serde_json::Value,
}

/// Everything asked, and whether all of it was well.
#[derive(Clone, Debug, Serialize)]
pub struct Health {
    pub well: bool,
    pub checks: Vec<Check>,
}

/// What is wrong, where anything is.
pub async fn look_at(tx: &mut Tx) -> Result<Health> {
    let checks = vec![
        has_something_on_it(tx).await?,
        the_last_build(tx).await?,
        work_nobody_finished(tx).await?,
        a_sweep_that_is_late(tx).await?,
    ];

    Ok(Health {
        well: checks.iter().all(|check| check.well),
        checks,
    })
}

/// A site with nothing published is not broken — it is what an installation
/// looks like on its first day. It is worth saying, and not worth alarming
/// about, so it is a check that answers well either way and carries the
/// number.
async fn has_something_on_it(tx: &mut Tx) -> Result<Check> {
    let (published, last): (i64, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "select count(*), max(published_at) from writings
          where state = 'published' and deleted_at is null",
    )
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(Check {
        what: "site.has_pages",
        well: true,
        detail: serde_json::json!({ "published": published, "last": last }),
    })
}

/// A design that would not build is the one somebody is looking for when they
/// ask why the site did not change.
async fn the_last_build(tx: &mut Tx) -> Result<Check> {
    let broken: Option<(String, String)> = sqlx::query_as(
        "select name, coalesce(went_wrong, '') from changes
          where at = 'broken' order by updated_at desc limit 1",
    )
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(Check {
        what: "design.last_build",
        well: broken.is_none(),
        detail: broken.map_or(
            serde_json::Value::Null,
            |(name, went_wrong)| serde_json::json!({ "name": name, "went_wrong": went_wrong }),
        ),
    })
}

/// Work the queue has given up on.
///
/// The one that actually goes wrong, and the one the crate this replaces did
/// not ask: a dead job is a letter nobody received or a build nobody got, and
/// nothing anywhere says so unless somebody looks in the table.
async fn work_nobody_finished(tx: &mut Tx) -> Result<Check> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "select kind, count(*) from jobs where state = 'dead' group by kind order by kind",
    )
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    let dead: i64 = rows.iter().map(|(_, how_many)| how_many).sum();

    Ok(Check {
        what: "work.given_up_on",
        well: dead == 0,
        detail: serde_json::json!({
            "dead": dead,
            "kinds": rows
                .into_iter()
                .map(|(kind, how_many)| serde_json::json!({ "kind": kind, "dead": how_many }))
                .collect::<Vec<_>>(),
        }),
    })
}

/// How late is too late for something that happens on its own.
///
/// Twice its own interval. Once would be every schedule complaining every time
/// a tick was a second behind; a fixed number of minutes would be wrong for
/// both the five-minute sweep and the daily one.
const LATE_BY: i32 = 2;

/// A sweep that should have run and has not.
///
/// What this catches is the scheduler not running at all — which looks like
/// nothing at all until stock that was held for a checkout nobody paid for has
/// been held for a week.
async fn a_sweep_that_is_late(tx: &mut Tx) -> Result<Check> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "select kind, extract(epoch from (now() - next_at))::bigint from schedules
          where next_at < now() - make_interval(secs => every_seconds * $1)
          order by kind",
    )
    .bind(LATE_BY)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(Check {
        what: "work.on_a_timer",
        well: rows.is_empty(),
        detail: serde_json::json!({
            "late": rows
                .into_iter()
                .map(|(kind, seconds)| serde_json::json!({ "kind": kind, "late_by": seconds }))
                .collect::<Vec<_>>(),
        }),
    })
}
