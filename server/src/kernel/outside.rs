//! What something outside this crate adds to it at startup.
//!
//! The operator's own half is going to depend on this crate rather than patch
//! it, and needs a way in that is not a patch: a handful of endpoints, and a
//! kind of work the queue can be asked to do. One value carries both, and
//! [`AppState`](super::http::AppState) carries the value — empty by default,
//! which is this crate on its own.
use std::future::Future;
use std::pin::Pin;

use super::error::Result;
use super::http::AppState;
use super::http::Endpoint;
use super::queue::Job;

/// What running a job of an outside kind gives back.
pub type JobFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// Runs a job of a kind nothing in this crate declares. Takes the same two
/// things [`crate::jobs::run`] does, and answers the same way.
pub type JobFn = for<'a> fn(&'a AppState, &'a Job) -> JobFuture<'a>;

/// Endpoints and job kinds handed in from outside this crate.
#[derive(Default)]
pub struct Outside {
    pub endpoints: Vec<Endpoint>,
    /// A kind of work, paired with what runs it. Checked against every kind
    /// this crate already runs before a worker ever claims one — a kind two
    /// things answer for is one the queue would hand to whichever matched
    /// first, silently.
    pub jobs: Vec<(&'static str, JobFn)>,
}

impl std::fmt::Debug for Outside {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outside")
            .field("endpoints", &self.endpoints.len())
            .field(
                "jobs",
                &self.jobs.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
            )
            .finish()
    }
}
