//! Where the rows are.
//!
//! Two things: a connection somebody can start a transaction on, and the one
//! place a listing's order becomes SQL. Nothing here knows what a post is.
//!
//! **A transaction is a value, and writing is what you do inside one.** The
//! type is not a formality — the audit receipt and the queue take one, and
//! that is what makes "the record is written in the same transaction as the
//! change" a thing the compiler checks rather than a thing everybody
//! remembers.

pub mod walk;

use mavi_core::error::{Error, Result};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Postgres, Transaction};

pub use walk::Walk;

/// The pool, and what migrates it.
#[derive(Clone, Debug)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    /// Opens the pool. The address is handed in, never read here — see
    /// `mavi_core::ports`.
    pub async fn open(address: &str, most: u32) -> Result<Self> {
        let options: PgConnectOptions = address.parse().map_err(Error::internal)?;

        let pool = PgPoolOptions::new()
            .max_connections(most)
            .connect_with(options)
            .await
            .map_err(Error::internal)?;

        Ok(Self { pool })
    }

    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Runs what has not run. Applied at boot, so an installation that starts
    /// is an installation whose schema matches the binary that started it.
    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(Error::internal)
    }

    /// A transaction. Everything that writes takes one of these, and nothing
    /// writes without one.
    pub async fn begin(&self) -> Result<Tx> {
        Ok(Tx {
            inner: self.pool.begin().await.map_err(Error::internal)?,
        })
    }
}

/// One transaction.
///
/// Dropped without [`Tx::commit`], it rolls back — which is the right default:
/// a handler that returns early on a refusal has written nothing, without
/// having to remember to undo it.
#[derive(Debug)]
pub struct Tx {
    inner: Transaction<'static, Postgres>,
}

impl Tx {
    /// What a query runs on.
    pub fn conn(&mut self) -> &mut sqlx::PgConnection {
        &mut self.inner
    }

    pub async fn commit(self) -> Result<()> {
        self.inner.commit().await.map_err(Error::internal)
    }
}
