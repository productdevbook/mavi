//! Talking to the one database.
//!
//! There was a transaction that had said which site it was and a transaction
//! that had said it was the machine's own work, and the difference between
//! them was the whole of the tenancy: the first set `app.tenant_id` so that
//! row-level security would hand it one site's rows, the second set
//! `app.worker` to be handed everybody's. With one site there is one kind of
//! transaction, and it says nothing before it starts.
//!
//! What survives the collapse is the *type*, and it is worth saying why rather
//! than treating [`Tx`] as a wrapper that could be dropped next. `audit::record`
//! and `queue::enqueue` take `&mut Tx`, and that signature is what makes "the
//! receipt is written in the same transaction as the change" a thing the
//! compiler checks instead of a thing somebody remembers.
use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{PgConnection, Postgres, Transaction};

use super::error::{AppError, Result};

#[derive(Clone, Debug)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    pub async fn connect(url: &str, max_connections: u32) -> Result<Self> {
        Self::connect_as(url, max_connections, None).await
    }

    /// Row-level security has no effect on a superuser, so the application's
    /// role is not one, and a test that asks whether isolation holds has to ask
    /// it as somebody it applies to.
    pub async fn connect_as(url: &str, max_connections: u32, role: Option<&str>) -> Result<Self> {
        let role = role.map(str::to_owned);

        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .after_connect(move |conn, _| {
                let role = role.clone();
                Box::pin(async move {
                    if let Some(role) = role {
                        // From configuration, never from a request; `set role`
                        // takes no parameter.
                        sqlx::query(&format!("set role {role}"))
                            .execute(conn)
                            .await?;
                    }
                    Ok(())
                })
            })
            .connect(url)
            .await?;

        Ok(Self { pool })
    }

    /// This crate's own migrations.
    ///
    /// Told not to object to versions it does not know: everything that has
    /// run is recorded in one table, so once a crate outside this one has
    /// carried its own in, they are sitting there — and a process that
    /// refused to start because of them would be a machine that came up once
    /// and never again.
    pub async fn migrate(&self) -> Result<()> {
        let mut ours = sqlx::migrate!("./migrations");
        ours.set_ignore_missing(true);
        ours.run(&self.pool).await.map_err(sqlx::Error::from)?;
        Ok(())
    }

    /// Runs a [`Migrator`](sqlx::migrate::Migrator) that is not this crate's
    /// own — what an outside crate carries in on [`Outside`](super::outside::Outside).
    ///
    /// sqlx tracks every migration it has run in one `_sqlx_migrations`
    /// table, whoever it belongs to; there is no way to give an outside
    /// crate's migrations a table of their own. So this crate's versions and
    /// an outside crate's must never collide, and this asks sqlx not to
    /// object that it sees versions in that table an outside `Migrator` never
    /// declared — this crate's own, applied first.
    /// A version that collides is refused here rather than by sqlx three
    /// steps later: what it says then is that a checksum does not match, which
    /// reads as a corrupted migration rather than as two crates having both
    /// called something `1`.
    pub async fn migrate_with(&self, mut migrator: sqlx::migrate::Migrator) -> Result<()> {
        let ours = sqlx::migrate!("./migrations");
        let highest = ours.iter().map(|one| one.version).max().unwrap_or(0);

        if let Some(clash) = migrator.iter().find(|one| one.version <= highest) {
            return Err(AppError::Bug(Box::leak(
                format!(
                    "an outside migration is numbered {} and this crate's own go up to {highest}: \
                     number them above that — a timestamp is what most do",
                    clash.version
                )
                .into_boxed_str(),
            )));
        }

        migrator.set_ignore_missing(true);
        migrator.run(&self.pool).await.map_err(sqlx::Error::from)?;
        Ok(())
    }

    /// A transaction. Nothing is said to the database before the first
    /// statement of it: what used to be said here was which site was asking.
    pub async fn begin(&self) -> Result<Tx> {
        Ok(Tx {
            tx: self.pool().begin().await?,
        })
    }

    /// The pool underneath, for a crate outside this one that has to reach
    /// the database in a way nothing here does yet.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

/// One transaction, and the handle a change and its audit row share.
#[derive(Debug)]
pub struct Tx {
    tx: Transaction<'static, Postgres>,
}

impl Tx {
    pub fn conn(&mut self) -> &mut PgConnection {
        &mut self.tx
    }

    pub async fn commit(self) -> Result<()> {
        Ok(self.tx.commit().await?)
    }
}
