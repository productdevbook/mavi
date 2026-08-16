//! What happens on its own, and how often.
//!
//! Not a cron expression. What anything here needs is "about this often", and
//! a schedule nobody can read is a schedule nobody notices is wrong.
//!
//! **One tick is one process's.** Two workers both deciding it is time to put
//! stock back is the sweep running twice; two both deciding it is time to
//! charge somebody is worse. So a tick is claimed by the same statement that
//! moves it forward — an `update ... returning` — and whichever transaction
//! commits first is the one that got it. The other re-reads the row after the
//! first commits, finds the next time in the future, and matches nothing.
//!
//! No lock is held between the claim and the work, and that is deliberate: a
//! worker that dies holding a lock is a schedule that stops for everybody, and
//! a worker that dies after claiming is one tick nobody did.

use std::time::Duration;

use mavi_core::error::{Error, Result};
use mavi_db::Tx;
use sqlx::Row;

/// One thing that happens on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Often {
    /// The kind of work to queue when it is due. The same name the queue
    /// knows, so a schedule for work nothing runs is refused where it is
    /// queued rather than accepted here.
    pub kind: &'static str,
    pub every: Duration,
}

impl Often {
    #[must_use]
    pub const fn new(kind: &'static str, every: Duration) -> Self {
        Self { kind, every }
    }

    /// Every so many minutes, which is how everything here is written.
    #[must_use]
    pub const fn minutes(kind: &'static str, how_many: u64) -> Self {
        Self::new(kind, Duration::from_mins(how_many))
    }
}

/// Makes sure every schedule has a row, and that its interval is what the code
/// says it is.
///
/// Called at every start rather than in a migration: how often something
/// happens is a fact about the code, and a migration that carries it is a
/// second copy that drifts. A schedule whose name has gone is left alone —
/// deleting it here would mean a rename in one process wiping the row while
/// another still has the old name.
pub async fn keep(tx: &mut Tx, all: &[Often]) -> Result<()> {
    for one in all {
        let seconds = i64::try_from(one.every.as_secs()).unwrap_or(i64::MAX);

        sqlx::query(
            "insert into schedules (kind, every_seconds, next_at)
             values ($1, $2, now())
             on conflict (kind) do update set every_seconds = excluded.every_seconds",
        )
        .bind(one.kind)
        .bind(seconds)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;
    }

    Ok(())
}

/// What is due now, claimed by this process.
///
/// Whatever comes back is this process's to queue, and has already been moved
/// forward — so a second process asking at the same moment gets an empty list
/// rather than the same work.
pub async fn due(tx: &mut Tx, all: &[Often]) -> Result<Vec<String>> {
    let names: Vec<String> = all.iter().map(|one| one.kind.to_owned()).collect();

    let rows = sqlx::query(
        "update schedules
            set next_at = now() + make_interval(secs => every_seconds),
                last_at = now()
          where kind = any($1) and next_at <= now()
         returning kind",
    )
    .bind(&names)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter()
        .map(|row| row.try_get("kind").map_err(Error::internal))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn how_often_is_said_in_something_a_person_reads() {
        let sweep = Often::minutes("shop.put-back", 5);

        assert_eq!(sweep.every, Duration::from_mins(5));
    }
}
