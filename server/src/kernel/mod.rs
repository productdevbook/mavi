//! What every domain is built out of.
//!
//! The router and its guards, the database and its two kinds of connection,
//! authorization, the queue, the clock, the words a refusal is said in. A
//! domain here is a folder of handlers and a table; everything it needs to be
//! a domain is in this module.
pub mod audit;
pub mod authz;
pub mod browser;
pub mod builder;
pub mod clock;
pub mod crypto;
pub mod db;
pub mod domain;
pub mod error;
pub mod events;
pub mod http;
pub mod mailer;
pub mod metrics;
pub mod money;
pub mod openapi;
pub mod outbound;
pub mod outside;
pub mod page;
pub mod password;
pub mod payments;
pub mod queue;
pub mod ratelimit;
pub mod retention;
pub mod say;
pub mod scheduler;
pub mod secret;
pub mod storage;
pub mod tenant;
pub mod token;
pub mod totp;
pub mod transcoder;
pub mod trash;
pub mod types;
pub mod typescript;
pub mod webhook;
pub mod worker;

pub use error::{AppError, Code, Result};
pub use tenant::TenantId;
