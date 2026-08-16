//! One installation, running.
//!
//! Everything above this is a library that answers questions. This is the
//! process: it reads what it has been told **once, here, at the edge**, opens
//! the database, applies whatever migrations have not run, and then does two
//! things at the same time — answers requests, and works through whatever was
//! queued behind them.
//!
//! Nothing below this file reads the environment. That is the whole reason
//! configuration is a value handed in rather than something a constructor goes
//! looking for: two of anything in one process would otherwise get one of them
//! wrong, and a test would inherit whatever the machine happened to have.

mod config;
mod doing;
mod who;

use std::sync::Arc;

use mavi_core::error::{Error, Result};
use mavi_core::ports::Files;
use mavi_db::Db;
use mavi_files::InADirectory;
use mavi_work::Queue;
use tokio::net::TcpListener;
use tokio::signal;

use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    let told = Config::from_the_environment()?;

    // Said out loud before anything else happens. A process that has read its
    // configuration and is about to act on it is the last moment where a
    // person can see what it thinks it was told.
    println!("{}", told.as_said());

    let db = Db::open(&told.database, told.at_most_connections).await?;
    db.migrate().await?;

    let files: Arc<dyn Files> = Arc::new(InADirectory::at(&told.files));
    let queue = Queue::of(&mavi_everything::work());

    let router =
        mavi_everything::mounted::site(&db, &files, who::whoever_holds(db.clone())).into_router();

    let listener = TcpListener::bind(&told.listen)
        .await
        .map_err(Error::internal)?;

    println!("answering on {}", told.listen);

    // The worker is a task beside the server rather than a second process, and
    // that is a decision rather than a convenience: a queue nobody runs is a
    // queue that fills up quietly, and one installation should not need two
    // things started in the right order to send a letter.
    let working = tokio::spawn(doing::keep_working(db.clone(), queue, told.worker.clone()));

    axum::serve(listener, router)
        .with_graceful_shutdown(asked_to_stop())
        .await
        .map_err(Error::internal)?;

    // Asked to stop, so the worker is asked too, and what it is holding is
    // left with a lapsed lease rather than half done: another worker takes it
    // when the lease runs out, which is what the lease is for.
    working.abort();

    println!("stopped");

    Ok(())
}

/// What a process is told to stop by.
///
/// Both of them: `SIGTERM` is what a container gets and `SIGINT` is what a
/// person types. Answering only the second is how a rollout waits thirty
/// seconds and then kills the process in the middle of a request.
async fn asked_to_stop() {
    let interrupt = async {
        signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut it) = signal::unix::signal(signal::unix::SignalKind::terminate()) {
            it.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}
