//! Work that happens after the answer.
//!
//! A letter to send, a page to build, a video to make smaller, a table to
//! sweep. One table, claimed with `for update skip locked` and a lease: two
//! workers reaching for the same row is one of them getting it, and a worker
//! that dies holding one loses it when the lease runs out rather than keeping
//! it for ever.
//!
//! Two things here are not how the queue this replaces did it.
//!
//! **A kind of work is declared.** Queueing a kind nothing runs is refused
//! where it is queued, rather than accepted and left in the table for as long
//! as the table exists. The way that happens is not carelessness — it is a
//! handler being renamed, or a domain being taken out, while something else
//! still queues its name.
//!
//! **Finishing a job says who is finishing it.** The queue this replaces
//! checked the claim when a worker asked to keep it and not when it said the
//! work was done, so a worker whose lease had lapsed could mark done a job
//! another worker was in the middle of. The second worker then failed it, the
//! row went back to ready, and the work ran a third time — from one slow
//! worker and no error anywhere.

pub mod pool;
pub mod timer;

pub use pool::{PoolConfig, ProcessLimits};

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::{Db, Tx};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

pub use timer::{Often, due, keep};

pub const NOTHING_RUNS_WORK_LIKE_THAT: &str = "nothing_runs_work_like_that";

/// How long a claim is good for. Work that runs longer says so while it runs,
/// with [`Queue::still_working`].
pub const LEASE: i64 = 300;

/// One kind of work, and how many times it is worth trying.
///
/// Tries are the kind's rather than the row's: a letter that a mail host
/// refused four times is worth another go tomorrow, and a video that killed
/// the transcoder twice is not. Keeping it here means the number is one thing
/// to change rather than one per row already queued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kind {
    pub name: &'static str,
    pub at_most_tries: i32,
}

impl Kind {
    #[must_use]
    pub const fn new(name: &'static str, at_most_tries: i32) -> Self {
        Self {
            name,
            at_most_tries,
        }
    }
}

/// Everything this installation runs.
///
/// Held by the queue rather than looked up when a job is taken, so a kind that
/// nothing runs cannot be queued at all — which is the difference between a
/// mistake somebody sees the moment they make it and a row that sits in the
/// table until somebody wonders what it is.
#[derive(Clone, Debug, Default)]
pub struct Queue {
    kinds: BTreeMap<&'static str, Kind>,
}

/// One job, as the worker running it sees it.
#[derive(Clone, Debug)]
pub struct Job {
    pub id: Uuid,
    pub kind: String,
    pub payload: serde_json::Value,
    /// Counting this one. A job on its first run says one.
    pub tries: i32,
}

/// What happened when a worker said it was finished.
///
/// `Somebody else's` is not an error to shout about — it is the ordinary end
/// of a worker that stalled long enough for its lease to lapse — but it is
/// never silence either: the work it did has been done twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    /// The job was still theirs, and is finished.
    Theirs,
    /// The lease had lapsed and somebody else holds it now. Nothing was
    /// written.
    SomebodyElses,
}

impl Queue {
    #[must_use]
    pub fn of(kinds: &[Kind]) -> Self {
        Self {
            kinds: kinds.iter().map(|kind| (kind.name, *kind)).collect(),
        }
    }

    #[must_use]
    pub fn runs(&self, name: &str) -> Option<Kind> {
        self.kinds.get(name).copied()
    }

    /// Queues work **in the transaction that is making the change it belongs
    /// to**. That is the whole reason this takes a [`Tx`]: a letter queued
    /// beside an order that then rolls back is a letter about an order nobody
    /// placed.
    pub async fn add(
        &self,
        tx: &mut Tx,
        kind: &str,
        payload: &impl Serialize,
        at: Option<DateTime<Utc>>,
    ) -> Result<Uuid> {
        if self.runs(kind).is_none() {
            return Err(Error::invalid(
                Say::of(NOTHING_RUNS_WORK_LIKE_THAT).with("kind", &kind),
            ));
        }

        let payload = serde_json::to_value(payload).map_err(Error::internal)?;

        let row = sqlx::query(
            "insert into jobs (id, kind, payload, run_at)
             values ($1, $2, $3, coalesce($4, now()))
             returning id",
        )
        .bind(Uuid::now_v7())
        .bind(kind)
        .bind(payload)
        .bind(at)
        .fetch_one(tx.conn())
        .await
        .map_err(Error::internal)?;

        Ok(row.get("id"))
    }

    /// Takes one job of the kinds this worker runs, skipping whatever another
    /// worker is holding.
    ///
    /// `skip locked` is what lets a second worker exist at all: two of them
    /// reaching for the same row do not queue behind each other, they take
    /// different rows.
    pub async fn take(&self, db: &Db, worker: &str, kinds: &[String]) -> Result<Option<Job>> {
        let row = sqlx::query(
            "update jobs
                set state = 'running',
                    claimed_until = now() + make_interval(secs => $1),
                    claimed_by = $2,
                    tries = tries + 1
              where id = (
                    select id from jobs
                     where kind = any($3)
                       and (
                            (state = 'ready' and run_at <= now())
                         -- A worker that died holding one does not keep it.
                         or (state = 'running' and claimed_until < now())
                       )
                     order by run_at
                     for update skip locked
                     limit 1
              )
             returning id, kind, payload, tries",
        )
        .bind(f64::from(i32::try_from(LEASE).unwrap_or(i32::MAX)))
        .bind(worker)
        .bind(kinds)
        .fetch_optional(db.pool())
        .await
        .map_err(Error::internal)?;

        Ok(row.map(|row| Job {
            id: row.get("id"),
            kind: row.get("kind"),
            payload: row.get("payload"),
            tries: row.get("tries"),
        }))
    }

    /// Says the work is still running, so the lease does not lapse under it.
    pub async fn still_working(&self, db: &Db, job: Uuid, worker: &str) -> Result<Ended> {
        self.if_still_theirs(
            db,
            "update jobs
                set claimed_until = now() + make_interval(secs => $3)
              where id = $1 and claimed_by = $2 and state = 'running'",
            job,
            worker,
            Some(f64::from(i32::try_from(LEASE).unwrap_or(i32::MAX))),
        )
        .await
    }

    /// Done.
    pub async fn done(&self, db: &Db, job: Uuid, worker: &str) -> Result<Ended> {
        self.if_still_theirs(
            db,
            "update jobs
                set state = 'done', claimed_until = null, finished_at = now()
              where id = $1 and claimed_by = $2 and state = 'running'",
            job,
            worker,
            None,
        )
        .await
    }

    /// Puts the job back, or gives up on it once it has been tried as often as
    /// its kind is worth trying.
    ///
    /// A dead job stays in the table. What failed and why is the thing
    /// somebody asks about afterwards, and deleting it answers nothing.
    pub async fn failed(&self, db: &Db, job: &Job, worker: &str, why: &str) -> Result<Ended> {
        let at_most = self.runs(&job.kind).map_or(1, |kind| kind.at_most_tries);
        let again = backoff(job.tries);

        let rows = sqlx::query(
            "update jobs
                set state = case when $3::int >= $4::int then 'dead' else 'ready' end,
                    claimed_until = null,
                    claimed_by = null,
                    went_wrong = $5,
                    run_at = now() + make_interval(secs => $6),
                    finished_at = case when $3::int >= $4::int then now() end
              where id = $1 and claimed_by = $2 and state = 'running'",
        )
        .bind(job.id)
        .bind(worker)
        .bind(job.tries)
        .bind(at_most)
        .bind(why)
        .bind(f64::from(
            i32::try_from(again.num_seconds()).unwrap_or(i32::MAX),
        ))
        .execute(db.pool())
        .await
        .map_err(Error::internal)?;

        Ok(if rows.rows_affected() == 0 {
            Ended::SomebodyElses
        } else {
            Ended::Theirs
        })
    }

    async fn if_still_theirs(
        &self,
        db: &Db,
        sql: &str,
        job: Uuid,
        worker: &str,
        seconds: Option<f64>,
    ) -> Result<Ended> {
        let mut query = sqlx::query(sql).bind(job).bind(worker);

        if let Some(seconds) = seconds {
            query = query.bind(seconds);
        }

        let rows = query.execute(db.pool()).await.map_err(Error::internal)?;

        Ok(if rows.rows_affected() == 0 {
            Ended::SomebodyElses
        } else {
            Ended::Theirs
        })
    }
}

/// Doubling, from a second, up to an hour.
///
/// A failure that is somebody else being down is not helped by asking again
/// immediately, and a hundred jobs all failing at once must not become a
/// hundred requests a second at whoever is already struggling.
#[must_use]
pub fn backoff(tries: i32) -> Duration {
    let exponent = u32::try_from(tries).unwrap_or(0).min(12);
    let seconds = 2_i64.saturating_pow(exponent);

    Duration::seconds(seconds.min(3600))
}

/// What a job carries, for a kind that carries nothing.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Nothing {}

#[cfg(test)]
mod tests {
    use super::*;

    const SEND: Kind = Kind::new("mail.send", 5);
    const SWEEP: Kind = Kind::new("forms.sweep", 1);

    #[test]
    fn a_kind_nothing_runs_is_not_a_kind() {
        let queue = Queue::of(&[SEND, SWEEP]);

        assert_eq!(queue.runs("mail.send"), Some(SEND));
        assert_eq!(queue.runs("mail.dispatch"), None);
    }

    #[test]
    fn waiting_doubles_and_then_stops_doubling() {
        assert_eq!(backoff(0).num_seconds(), 1);
        assert_eq!(backoff(1).num_seconds(), 2);
        assert_eq!(backoff(5).num_seconds(), 32);

        // A hundred jobs failing at once must not become a hundred requests a
        // second at whoever is already down, and an hour is where doubling is
        // allowed to stop.
        assert_eq!(backoff(30).num_seconds(), 3600);
        assert_eq!(backoff(-1).num_seconds(), 1);
    }
}
