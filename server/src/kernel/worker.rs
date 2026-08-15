//! The loop that actually takes work off the queue.
//!
//! Everything in this build that is not a request is a row in `jobs` waiting
//! for something to pick it up. Without this, mail is written and never sent, a
//! publish is asked for and never built, and a transfer waits for ever — all of
//! it looking, from the panel, like something that is about to happen.

use std::time::Duration;

use tokio::sync::watch;

use super::error::Result;
use super::http::AppState;

/// How long to wait after finding nothing.
///
/// Short enough that a letter written now goes out now, long enough that an
/// idle machine is not asking Postgres five times a second for ever.
const NOTHING_TO_DO: Duration = Duration::from_secs(2);

/// How long to wait after something went wrong here rather than in the work.
/// A database that has gone away comes back, and hammering it while it does is
/// how a machine that was nearly well stays unwell.
const SOMETHING_IS_WRONG: Duration = Duration::from_secs(10);

/// Takes work until it is told to stop, and finishes what it is holding first.
///
/// A job that fails is the queue's business — it counts the attempt, waits
/// longer each time, and gives up into the dead letter. What is caught here is
/// the loop itself failing: nothing else would say so.
pub async fn work(state: AppState, name: String, mut stop: watch::Receiver<bool>) {
    tracing::info!(worker = %name, "taking work");

    loop {
        if *stop.borrow() {
            break;
        }

        let took = crate::jobs::tick(&state, &name).await;

        let wait = match took {
            // Something was done: straight back for the next one, because a
            // queue with a hundred rows in it should empty in seconds.
            Ok(true) => continue,
            Ok(false) => NOTHING_TO_DO,
            Err(why) => {
                tracing::error!(worker = %name, error = %why, "a worker could not take work");
                SOMETHING_IS_WRONG
            }
        };

        // Whichever comes first: the wait, or being told to stop. A pod that
        // has been asked to go should not sit out its idle wait first.
        tokio::select! {
            () = tokio::time::sleep(wait) => {}
            _ = stop.changed() => {}
        }
    }

    tracing::info!(worker = %name, "stopped taking work");
}

/// Puts the day's — and the minute's — work in the queue, for every site.
///
/// One process does this at a time, under the lock the scheduler holds, so two
/// replicas do not both decide it is time to reconcile payments.
pub async fn keep_time(state: AppState, mut stop: watch::Receiver<bool>) {
    /// How often to look at what is due. Everything scheduled says how often it
    /// wants to run; this is only how often that is checked.
    const LOOK: Duration = Duration::from_mins(1);

    tracing::info!("keeping time");

    loop {
        if *stop.borrow() {
            break;
        }

        if let Err(why) = tick(&state).await {
            tracing::error!(error = %why, "the scheduler could not put work in the queue");
        }

        tokio::select! {
            () = tokio::time::sleep(LOOK) => {}
            _ = stop.changed() => {}
        }
    }

    tracing::info!("stopped keeping time");
}

async fn tick(state: &AppState) -> Result<()> {
    crate::jobs::schedule_due(state).await
}

/// Fires once when the process is asked to stop, and again is not asked twice.
///
/// SIGTERM is what Kubernetes sends and Ctrl-C is what a person sends; both
/// mean the same thing here.
#[must_use]
pub fn asked_to_stop() -> watch::Receiver<bool> {
    let (say, hear) = watch::channel(false);

    tokio::spawn(async move {
        let interrupt = async {
            let _ = tokio::signal::ctrl_c().await;
        };

        #[cfg(unix)]
        let terminate = async {
            if let Ok(mut signal) =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            {
                signal.recv().await;
            }
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            () = interrupt => {}
            () = terminate => {}
        }

        tracing::info!("asked to stop");
        let _ = say.send(true);
    });

    hear
}
