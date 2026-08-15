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
use super::retention::Policy;
use super::scheduler::Every;

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
    /// Tables an outside crate owns. Run after this crate's own migrations
    /// and never before: theirs may reference a table this crate created, but
    /// nothing this crate does may ever come to depend on a table only an
    /// outside crate knows how to build.
    ///
    /// sqlx tracks every migration, whoever it belongs to, in one
    /// `_sqlx_migrations` table — there is no way to give an outside crate's
    /// migrations a table of their own. So an outside crate's version
    /// numbers must live outside this crate's own range (`server/migrations`
    /// is in the low thousands at most; nine digits is well clear of it) or
    /// a version meant as theirs is read back as one of ours with the wrong
    /// checksum, and migrating refuses to run at all.
    pub migrations: Option<sqlx::migrate::Migrator>,
    /// How often one of `jobs` runs on its own, rather than in answer to a
    /// request. Every kind named here must also be a kind in `jobs` —
    /// checked at startup, in [`crate::jobs::kinds`] — because a schedule for
    /// a job nobody handed in is work that queues and nothing ever claims.
    pub schedules: Vec<(&'static str, Every)>,
    /// Retention policies for tables an outside crate owns, held to the same
    /// rule this crate's own [`POLICIES`](super::retention::POLICIES) are:
    /// every one must name a sweep that is a real job kind, checked wherever
    /// this crate checks its own.
    pub policies: Vec<Policy>,
}

impl std::fmt::Debug for Outside {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outside")
            .field("endpoints", &self.endpoints.len())
            .field(
                "jobs",
                &self.jobs.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
            )
            .field("migrations", &self.migrations.is_some())
            .field("schedules", &self.schedules)
            .field(
                "policies",
                &self.policies.iter().map(|p| p.table).collect::<Vec<_>>(),
            )
            .finish()
    }
}
