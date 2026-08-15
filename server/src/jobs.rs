//! Every kind of work there is, and what runs it.
//!
//! One list: what the queue may be asked to do, and which function does it. A
//! kind nothing here knows is a job that would sit in the queue for ever, so
//! this file is the one place a new one is added.
use crate::analytics;
use crate::content;
use crate::flows;
use crate::forms;
use crate::health;
use crate::housekeeping::{self, SweepAudit, SweepDeliveries, SweepReports, SweepSessions};
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::AppState;
use crate::kernel::outside::Outside;
use crate::kernel::queue::{self, Job, Task};
use crate::mail;
use crate::media;
use crate::publishing;
use crate::shop;
use crate::trash;
use crate::webhooks::{self, Deliver, Dispatch};

/// Every kind of work this build can run. A task with no arm here is a task
/// nothing picks up, and the match is where that becomes obvious.
///
/// `outside`'s own kinds are appended, each checked against everything named
/// so far: a kind two things claim is a bug caught the moment anything asks
/// what the queue may be told to do, not a job that silently goes to whichever
/// answered first.
#[must_use]
pub fn kinds(outside: &Outside) -> Vec<String> {
    let mut all = webhooks::kinds();
    all.push(<forms::Sweep as Task>::KIND.to_owned());
    all.extend(housekeeping::kinds());
    all.extend(media::kinds());
    all.extend(health::kinds());
    all.extend(shop::kinds());
    all.extend(mail::kinds());
    all.extend(flows::kinds());
    all.extend(publishing::kinds());
    all.extend(analytics::kinds());
    all.extend(content::kinds());
    all.extend(trash::kinds());

    for (kind, _) in &outside.jobs {
        assert!(
            !all.iter().any(|already| already == kind),
            "{kind} is a job kind claimed by more than one thing"
        );
        all.push((*kind).to_owned());
    }

    all
}

/// What runs on a schedule, and how often.
///
/// Everything that is not started by something happening is here, because a
/// task nothing schedules is a task that never runs — and it would look, from
/// the panel, exactly like one that is about to.
pub async fn schedule_due(state: &AppState) -> Result<()> {
    use crate::kernel::scheduler::{Every, every};

    // Late the moment they are due.
    every::<content::PublishDue>(state, Every::Minute).await?;
    every::<shop::ReleaseHolds>(state, Every::Minute).await?;

    // Often enough to matter, not often enough to be noise.
    every::<shop::DropStuck>(state, Every::Hour).await?;
    every::<shop::Reconcile>(state, Every::Hour).await?;

    // Once a day is what these are for: sweeping, counting, and looking at
    // addresses somebody else's DNS answers for.
    every::<forms::Sweep>(state, Every::Day).await?;
    every::<housekeeping::SweepSessions>(state, Every::Day).await?;
    every::<housekeeping::SweepAudit>(state, Every::Day).await?;
    every::<housekeeping::SweepDeliveries>(state, Every::Day).await?;
    every::<housekeeping::SweepReports>(state, Every::Day).await?;
    every::<shop::SweepOrders>(state, Every::Day).await?;
    every::<shop::WarnOnLowStock>(state, Every::Day).await?;
    every::<mail::SweepLog>(state, Every::Day).await?;
    every::<analytics::SweepMarks>(state, Every::Day).await?;
    every::<trash::Empty>(state, Every::Day).await?;
    every::<health::CheckDomains>(state, Every::Day).await?;

    for (kind, how_often) in &state.outside.schedules {
        crate::kernel::scheduler::every_named(state, kind, *how_often).await?;
    }

    Ok(())
}

/// A schedule naming a kind nothing handed in as a job would sit in the queue
/// forever, silently claimed by nothing — this is where that becomes a
/// refusal at startup instead.
///
/// # Panics
///
/// If a schedule names a kind that is not also one of `outside.jobs`.
pub fn assert_schedules_are_runnable(outside: &Outside) {
    for (kind, _) in &outside.schedules {
        assert!(
            outside.jobs.iter().any(|(job, _)| job == kind),
            "{kind} is scheduled but was never handed in as a job"
        );
    }
}

pub async fn run(state: &AppState, job: &Job) -> Result<()> {
    let tenant = job.tenant_id.ok_or(AppError::Bug(
        "this work belongs to a tenant and arrived without one",
    ))?;

    match job.kind.as_str() {
        Dispatch::KIND => webhooks::dispatch(state, tenant, &job.task::<Dispatch>()?).await,
        Deliver::KIND => {
            webhooks::deliver(state, tenant, &job.task::<Deliver>()?, job.attempts).await
        }
        forms::Sweep::KIND => forms::sweep(state, tenant).await.map(|_| ()),
        media::HandOver::KIND => media::hand_over(state, tenant, job).await,
        health::CheckDomains::KIND => health::run(state, tenant, job).await,
        SweepSessions::KIND => housekeeping::sweep_sessions(state, tenant)
            .await
            .map(|_| ()),
        SweepAudit::KIND => housekeeping::sweep_audit(state, tenant).await.map(|_| ()),
        SweepDeliveries::KIND => housekeeping::sweep_deliveries(state, tenant)
            .await
            .map(|_| ()),
        SweepReports::KIND => housekeeping::sweep_reports(state, tenant).await.map(|_| ()),
        shop::ReleaseHolds::KIND => shop::release_holds(state, tenant).await.map(|_| ()),
        shop::DropStuck::KIND => shop::drop_stuck(state, tenant).await.map(|_| ()),
        shop::WarnOnLowStock::KIND => shop::warn_on_low_stock(state, tenant).await.map(|_| ()),
        shop::SweepOrders::KIND => shop::sweep_orders(state, tenant).await.map(|_| ()),
        shop::Reconcile::KIND => shop::reconcile(state, tenant).await.map(|_| ()),
        mail::SendBatch::KIND => mail::send_batch(state, tenant, &job.task::<mail::SendBatch>()?)
            .await
            .map(|_| ()),
        mail::SweepLog::KIND => mail::sweep_log(state, tenant).await.map(|_| ()),
        mail::Deliver::KIND => mail::deliver(state, tenant, &job.task::<mail::Deliver>()?).await,
        flows::Start::KIND => flows::start(state, tenant, &job.task::<flows::Start>()?)
            .await
            .map(|_| ()),
        flows::Step::KIND => flows::step(state, tenant, &job.task::<flows::Step>()?).await,
        analytics::SweepMarks::KIND => analytics::sweep_marks(state, tenant).await.map(|_| ()),
        content::PublishDue::KIND => content::publish_due(state, tenant).await.map(|_| ()),
        trash::Empty::KIND => trash::empty(state, tenant).await.map(|_| ()),
        publishing::Build::KIND => {
            publishing::build(state, tenant, &job.task::<publishing::Build>()?).await
        }
        kind => match state
            .outside
            .jobs
            .iter()
            .find(|(claimed, _)| *claimed == kind)
        {
            Some((_, run)) => run(state, job).await,
            None => Err(AppError::Bug("a kind of work nothing knows how to run")),
        },
    }
}

/// Takes one piece of work if there is any, and says whether it found one.
pub async fn tick(state: &AppState, worker: &str) -> Result<bool> {
    tick_within(state, worker, None).await
}

/// The same, for a worker that has been given one site to look after.
pub async fn tick_within(
    state: &AppState,
    worker: &str,
    tenant: Option<crate::kernel::TenantId>,
) -> Result<bool> {
    let Some(job) = queue::claim_within(&state.db, worker, &kinds(&state.outside), tenant).await?
    else {
        return Ok(false);
    };

    let outcome = run(state, &job).await;

    crate::kernel::metrics::job_ran(&job.kind, outcome.is_ok());

    match outcome {
        Ok(()) => queue::complete(&state.db, job.id).await?,
        Err(failure) => {
            queue::fail(
                &state.db,
                job.id,
                &failure.to_string(),
                queue::backoff(job.attempts),
            )
            .await?;
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    /// A kind in the list that nothing runs is work that queues and fails
    /// forever, silently, because the arm that catches it is the one that says
    /// nothing knows how. It happened once here already, to the mail batch.
    #[tokio::test]
    async fn everything_the_queue_takes_is_something_that_runs() {
        use crate::kernel::db::Db;
        use crate::kernel::http::AppState;
        use crate::kernel::queue::Job;

        let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
            return;
        };

        let db = Db::connect(&url, 1).await.expect("connect");
        let state = AppState::new(db);

        for kind in super::kinds(&state.outside) {
            let job = Job {
                id: uuid::Uuid::now_v7(),
                tenant_id: Some(crate::kernel::TenantId(uuid::Uuid::nil())),
                kind: kind.clone(),
                payload: serde_json::json!({}),
                attempts: 1,
                max_attempts: 5,
            };

            // What it does with a tenant that is not one is its own business;
            // what is being asked is only whether anything claims it at all.
            let outcome = super::run(&state, &job).await;

            assert!(
                !matches!(
                    outcome,
                    Err(crate::kernel::AppError::Bug(
                        "a kind of work nothing knows how to run"
                    ))
                ),
                "{kind} goes into the queue and nothing takes it out"
            );
        }
    }
}
