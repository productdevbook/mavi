//! Which site this is.
//!
//! Nothing decides. One installation is one site, so a request carries no
//! question about whose data it is looking at: the site is the one row there
//! is, and an address is only ever the address somebody typed.
//!
//! The id is still carried through everything below here, because the column
//! it names is still on every table that holds a site's data. When that goes,
//! this file goes with it.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::db::Db;
use super::error::{AppError, Result};
use super::say;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct TenantId(pub Uuid);

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// The one site, read from the one row.
///
/// Read per request rather than held anywhere: what it reads is a table on its
/// way out, and a cache would outlive the thing it caches.
///
/// Before `/api/setup` has run there is no row, and there is nothing this can
/// honestly answer with — the endpoint exists, the caller is nobody in
/// particular, and the machine is simply not a site yet. So it says that, and
/// says it as something a panel can put in somebody's own language.
pub async fn the_site(db: &Db) -> Result<TenantId> {
    let mut conn = db.operator().await?;
    conn.across_sites().await?;

    let found: Option<(Uuid,)> = sqlx::query_as("select id from tenants limit 1")
        .fetch_optional(conn.conn())
        .await?;

    conn.commit().await?;

    found.map(|(id,)| TenantId(id)).ok_or(AppError::Conflict(
        say::THIS_MACHINE_IS_NOT_SET_UP_YET.into(),
    ))
}
