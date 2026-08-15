//! Talking to the one database, as one site or as the machine.
//!
//! Every site's rows live in the same tables with a `tenant_id`, and every one
//! of those tables has row-level security enabled and forced. A [`TenantConn`]
//! is a transaction that has said which site it is, so the database itself
//! refuses to hand it anything else; an [`OperatorConn`] is one that has said
//! it is the machine's own work, which is the only way to read across sites.
//!
//! A connection is asked for per request rather than held: the pool is one
//! pool, and how many sites exist has nothing to do with how much is open.
use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{PgConnection, Postgres, Transaction};

use super::error::Result;
use super::tenant::TenantId;

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

    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(sqlx::Error::from)?;
        Ok(())
    }

    pub async fn tenant(&self, tenant: TenantId) -> Result<TenantConn> {
        TenantConn::begin(self, tenant).await
    }

    pub async fn operator(&self) -> Result<OperatorConn> {
        OperatorConn::begin(self).await
    }

    pub(crate) fn pool(&self) -> &PgPool {
        &self.pool
    }
}

/// A transaction that has said which tenant is asking.
///
/// `set local` scopes the setting to the transaction, so a connection going
/// back to the pool carries nothing of whoever had it.
#[derive(Debug)]
pub struct TenantConn {
    tenant: TenantId,
    tx: Transaction<'static, Postgres>,
}

impl TenantConn {
    async fn begin(db: &Db, tenant: TenantId) -> Result<Self> {
        let mut tx = db.pool().begin().await?;

        sqlx::query("select set_config('app.tenant_id', $1::text, true)")
            .bind(tenant.0)
            .execute(&mut *tx)
            .await?;

        Ok(Self { tenant, tx })
    }

    #[must_use]
    pub fn tenant(&self) -> TenantId {
        self.tenant
    }

    pub fn conn(&mut self) -> &mut PgConnection {
        &mut self.tx
    }

    pub async fn commit(self) -> Result<()> {
        Ok(self.tx.commit().await?)
    }
}

/// Reaches across tenants, and is named so that every use of it can be found.
/// The tenant-scoped tables read as empty here.
#[derive(Debug)]
pub struct OperatorConn {
    tx: Transaction<'static, Postgres>,
}

impl OperatorConn {
    async fn begin(db: &Db) -> Result<Self> {
        Ok(Self {
            tx: db.pool().begin().await?,
        })
    }

    /// Reads rows belonging to sites, across every site at once.
    ///
    /// Only the machine's own screens and the queue do this, and only for the
    /// tables whose policy allows it — the setting is the same one the queue
    /// uses, so every table that can be read this way says so in its policy
    /// rather than leaving it to whoever opened the connection.
    pub async fn across_sites(&mut self) -> Result<()> {
        sqlx::query("select set_config('app.worker', 'on', true)")
            .execute(&mut *self.tx)
            .await?;

        Ok(())
    }

    /// Makes the rest of this transaction act as a site, for the one case that
    /// needs it: a site being made writes its first rows in the same
    /// transaction that made it, and until that commits there is no site for a
    /// separate connection to open.
    pub async fn provisioning_for(&mut self, tenant: TenantId) -> Result<()> {
        sqlx::query("select set_config('app.tenant_id', $1::text, true)")
            .bind(tenant.0)
            .execute(&mut *self.tx)
            .await?;

        Ok(())
    }

    pub fn conn(&mut self) -> &mut PgConnection {
        &mut self.tx
    }

    pub async fn commit(self) -> Result<()> {
        Ok(self.tx.commit().await?)
    }
}
