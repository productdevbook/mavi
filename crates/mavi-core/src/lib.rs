//! What every other crate is built out of.
//!
//! Nothing here knows what a post is, what a shop sells, or that a database
//! exists. It is the vocabulary: what a refusal is, what an id is, what money
//! is, how a list is walked, and what a host is asked to provide.
//!
//! The rule that keeps it that way: **a name that belongs to one thing a site
//! does does not appear in this crate.** A type here is one every domain
//! needs, or it is in the domain that needs it.

pub mod id;
pub mod money;
pub mod page;
pub mod ports;
pub mod say;

mod error;

pub use error::{Code, Error, Result};
pub use say::Say;
