//! Working through what was queued.
//!
//! One loop, taking one job at a time. What it does with each is decided by
//! the job's kind, and a kind this does not know how to do is a **failure that
//! says so** rather than a job quietly taken and dropped — the queue already
//! refuses to accept a kind nothing runs, so a kind arriving here that nothing
//! answers means the two lists have come apart, and that is worth an error
//! somebody can read.

use std::sync::Arc;
use std::time::Duration;

use mavi_core::error::Result;
use mavi_core::ports::{Builds, Files};
use mavi_db::Db;
use mavi_work::{Ended, Job, Queue};
use uuid::Uuid;

use crate::config::Worker;

/// Takes work until something stops the process.
pub async fn keep_working(
    db: Db,
    queue: Queue,
    files: Arc<dyn Files>,
    builds: Arc<dyn Builds>,
    worker: Worker,
) {
    let kinds: Vec<String> = mavi_everything::work()
        .iter()
        .map(|kind| kind.name.to_owned())
        .collect();

    loop {
        match queue.take(&db, &worker.named, &kinds).await {
            Ok(Some(job)) => {
                one(&db, &queue, files.as_ref(), builds.as_ref(), &worker, &job).await;
            }
            Ok(None) => tokio::time::sleep(worker.when_there_is_nothing).await,
            Err(wrong) => {
                // The database is not there, or is refusing. Said, and then
                // waited on: a loop that spins on an unreachable database is
                // a loop that fills a disk with the same line.
                tracing::error!(error = %wrong, "could not take work from queue");

                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

/// One job, and what happens to it afterwards.
///
/// The two answers are the queue's own: it is finished, or it failed and the
/// queue decides whether that is another go or the end of it. Neither is
/// written here, because "how many times is worth trying" belongs to the kind
/// rather than to the loop.
async fn one(
    db: &Db,
    queue: &Queue,
    files: &dyn Files,
    builds: &dyn Builds,
    worker: &Worker,
    job: &Job,
) {
    let went = doing(db, files, builds, job).await;

    let ended = match went {
        Ok(()) => queue.done(db, job.id, &worker.named).await,
        Err(wrong) => {
            tracing::error!(kind = %job.kind, error = %wrong, "job failed");

            queue
                .failed(db, job, &worker.named, &wrong.to_string())
                .await
        }
    };

    // Overtaken while it was working: its lease lapsed and somebody else has
    // the job now. Not an error to shout about — but never silence either,
    // because the work it just did has been done twice.
    if let Ok(Ended::SomebodyElses) = ended {
        tracing::warn!(
            kind = %job.kind,
            "job was taken by somebody else while this worker was doing it"
        );
    }
}

/// What each kind of work actually is.
async fn doing(db: &Db, files: &dyn Files, builds: &dyn Builds, job: &Job) -> Result<()> {
    match job.kind.as_str() {
        name if name == mavi_shop::PUT_BACK_WHAT_NOBODY_PAID_FOR.name => {
            put_back_what_nobody_paid_for(db).await
        }
        name if name == mavi_design::BUILD_A_LOOK.name => {
            build_a_look(db, files, builds, job).await
        }
        name if name == mavi_flows::SOMETHING_HAPPENED.name => {
            not_written_yet(name, "starting what a site arranged for an event")
        }
        name if name == mavi_flows::ONE_STEP.name => {
            not_written_yet(name, "running one step of a flow")
        }
        name => not_written_yet(name, "nothing here knows what this is"),
    }
}

/// One set of changes, built.
///
/// What was queued is an id and nothing else, so the whole of what is built
/// comes out of the database rather than out of the job — a payload written
/// last week is not a description of a design as it is now.
async fn build_a_look(db: &Db, files: &dyn Files, builds: &dyn Builds, job: &Job) -> Result<()> {
    let change = job.payload["change"]
        .as_str()
        .and_then(|change| Uuid::parse_str(change).ok())
        .ok_or_else(|| {
            mavi_core::error::Error::internal(std::io::Error::other(
                "a build was queued without saying what to build",
            ))
        })?;

    mavi_everything::building::build(db, files, builds, change).await?;

    Ok(())
}

/// A kind that is declared and has no hands yet.
///
/// Running a flow needs somewhere for a letter to go. That is a port, and it
/// arrives as an argument the day the hands are written rather than being
/// carried around empty until then.
///
/// An error rather than a shrug. The queue will try it again and then give up
/// on it, and what is left is a dead row that says which kind and why — which
/// is the only way anybody finds out that a domain queued work nothing does.
fn not_written_yet(kind: &str, what: &str) -> Result<()> {
    Err(mavi_core::error::Error::internal(std::io::Error::other(
        format!("{kind} is declared and not written yet: {what}"),
    )))
}

/// Stock held for a checkout nobody paid for.
///
/// The one kind that is entirely rows, so the one that can be written before
/// anything else exists. Every hold that has run out is put back — once, which
/// is the store's own rule, so a sweep that runs twice does not invent things
/// the shop does not have.
async fn put_back_what_nobody_paid_for(db: &Db) -> Result<()> {
    let mut tx = db.begin().await?;

    let orders: Vec<Uuid> = mavi_shop::store::holds_that_ran_out(&mut tx).await?;

    for order in orders {
        mavi_shop::store::put_back(&mut tx, order).await?;
    }

    tx.commit().await
}

/// Queues what is due, for as long as the process is up.
///
/// A tick claimed here is this process's: the claim and the moving-forward are
/// one statement, so two workers ticking at the same moment is one of them
/// getting it. What it does with a tick is put a job in the queue — the work
/// itself is the worker's, and a scheduler that did the work would be a second
/// place where a job runs without a lease.
pub async fn keep_time(db: Db, queue: Queue, worker: Worker) {
    let all = mavi_everything::on_a_timer();

    loop {
        match a_tick(&db, &queue, &all).await {
            Ok(queued) => {
                for kind in queued {
                    tracing::info!(kind = %kind, worker = %worker.named, "task is due, queued by worker");
                }
            }
            Err(wrong) => tracing::error!(error = %wrong, "could not ask what is due"),
        }

        // Often enough that the shortest schedule is not late by much, rarely
        // enough that an idle installation is not asking a database twice a
        // second what time it is.
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

/// One tick: what is due, and the work for it, in one transaction.
///
/// Both or neither. A tick moved forward with nothing queued behind it is a
/// sweep that is skipped for a whole interval and leaves no trace of having
/// been.
async fn a_tick(db: &Db, queue: &Queue, all: &[mavi_work::Often]) -> Result<Vec<String>> {
    let mut tx = db.begin().await?;

    let due = mavi_work::due(&mut tx, all).await?;

    for kind in &due {
        queue
            .add(&mut tx, kind, &serde_json::json!({}), None)
            .await?;
    }

    tx.commit().await?;

    Ok(due)
}
