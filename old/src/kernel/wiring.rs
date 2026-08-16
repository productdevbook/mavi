//! What the kernel needs to happen, and nothing about what does it.
//!
//! The kernel answers requests, takes work off a queue and writes down what
//! happened — and knows what none of that *is*. Where it needs something to
//! happen it declares the shape of it here and is handed one, the same way
//! [`Outside`](super::outside::Outside) hands in what a crate built on this one
//! adds. A kernel that reached for a domain instead would be a kernel no other
//! domain could be built on.
use std::future::Future;
use std::pin::Pin;

use axum::Router;
use uuid::Uuid;

use super::db::{Db, Tx};
use super::error::Result;
use super::http::{AppState, SignedInStudent};
use super::outside::Outside;

/// What one of these answers with. Boxed because a function pointer cannot
/// name the future an `async fn` returns.
pub type Answers<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// Takes one piece of work if there is any, and says whether it found one.
pub type TakesWork = for<'a> fn(&'a AppState, &'a str) -> Answers<'a, bool>;

/// Puts whatever is due into the queue.
pub type KeepsTime = for<'a> fn(&'a AppState) -> Answers<'a, ()>;

/// Queues what follows an event, in the transaction the event was written in.
pub type FollowsAnEvent = for<'a> fn(&'a mut Tx, Uuid) -> Answers<'a, ()>;

/// The name an event is counted under.
pub type NamesAnEvent = fn(&str) -> &'static str;

/// Whoever a student's token belongs to, if it is still good for anything.
pub type FindsAStudent = for<'a> fn(&'a Db, &'a str) -> Answers<'a, Option<SignedInStudent>>;

/// The description of what is served, including whatever `Outside` mounted.
pub type Describes = fn(&Outside) -> utoipa::openapi::OpenApi;

/// What is built on the kernel, handed to it.
///
/// Every field is empty by default, and empty is a kernel with nothing built on
/// it: it answers its own probes, takes no work and announces to nobody.
#[derive(Clone, Default)]
pub struct Wiring {
    /// What answers anything that is not one of the endpoints. Mounted behind
    /// the same layer that identifies a caller, because whatever it serves is
    /// served to one.
    pub otherwise: Option<Router<AppState>>,
    /// What the worker loop calls until it is told to stop.
    pub takes_work: Option<TakesWork>,
    /// What the scheduler calls when it looks at what is due.
    pub keeps_time: Option<KeepsTime>,
    /// What is queued when an event is written down. In the same transaction as
    /// the event, so nothing is arranged for a change that then rolled back.
    pub follows_an_event: Vec<FollowsAnEvent>,
    /// Which name an event is counted under, for whoever is reading a graph.
    pub names_an_event: Option<NamesAnEvent>,
    /// How a student's cookie becomes somebody. Absent where nothing on this
    /// installation has students, and then a student's cookie is nobody.
    pub a_student: Option<FindsAStudent>,
    /// What `/openapi.json` answers with.
    pub describes: Option<Describes>,
}

impl std::fmt::Debug for Wiring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wiring")
            .field("otherwise", &self.otherwise.is_some())
            .field("takes_work", &self.takes_work.is_some())
            .field("keeps_time", &self.keeps_time.is_some())
            .field("follows_an_event", &self.follows_an_event.len())
            .field("names_an_event", &self.names_an_event.is_some())
            .field("a_student", &self.a_student.is_some())
            .field("describes", &self.describes.is_some())
            .finish()
    }
}
