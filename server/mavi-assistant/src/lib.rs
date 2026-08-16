//! What an assistant can do here.
//!
//! An assistant is a caller. It signs in the way every other caller does, it
//! holds what its account holds, and what it may do is what that account may
//! do — so there is no list of tools in this crate, and that is the whole
//! design.
//!
//! The crate this replaces had one: seven hundred lines of hand-written tools,
//! each with its own query and its own idea of which grant it consumed. Every
//! one of them was a second copy of an endpoint the panel already had, and a
//! second copy is a place for the two to disagree. "Forbidden in the panel,
//! allowed over the assistant's door" was one mistake away at all times.
//!
//! So a tool **is** an endpoint. What is here turns the description this
//! installation already publishes into the shape an assistant expects, and
//! turns what an assistant sends back into the pieces an endpoint is called
//! with. Nothing in this file touches a database, a store, or HTTP: hand it a
//! description and it answers, which is what makes every rule in it something
//! a test can call.
//!
//! ## Names
//!
//! An endpoint is called `writings.throw-away`. The protocol wants a name of
//! letters, digits, underscores and dashes, so a tool is `writings_throw_away`
//! — and because that mapping is not reversible by guessing, both directions
//! are worked out from the same list rather than by taking a name apart.

pub mod called;
pub mod schema;
pub mod talk;

pub use called::{Tool, asked_for, named, tools};
pub use schema::{pieces, takes};
pub use talk::{Answer, Asked, answered, came_back, not_a_method, refused, what_was_asked};

/// What this answers to `initialize`.
///
/// Not negotiated against what a client asks for: there is one shape served
/// here, so there is one version to claim. A client that wanted another is
/// better told plainly than handed something that is nearly it.
pub const PROTOCOL: &str = "2025-06-18";
