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
/// Two rows are read rather than one, so that "more than one" is something
/// this can see rather than something it silently picks from. Both of the
/// answers it cannot give are refusals a person can read:
///
/// - **No row.** `/api/setup` has not run. The endpoint exists and the caller
///   is nobody in particular; the machine is simply not a site yet.
/// - **Two rows.** The database was made by a build that served many, and the
///   `tenant_id` and the policies dividing them are still in the schema. There
///   is no honest way to choose, and choosing wrongly is one site's panel
///   showing another site's posts with nothing anywhere saying so. So it
///   refuses at the door, on every request, which is also how somebody finds
///   out their database has to be split before this version can serve it.
pub async fn the_site(db: &Db) -> Result<TenantId> {
    let mut conn = db.operator().await?;
    conn.across_sites().await?;

    let found: Vec<(Uuid,)> = sqlx::query_as("select id from tenants limit 2")
        .fetch_all(conn.conn())
        .await?;

    conn.commit().await?;

    match found.as_slice() {
        [(id,)] => Ok(TenantId(*id)),
        [] => Err(AppError::Conflict(
            say::THIS_MACHINE_IS_NOT_SET_UP_YET.into(),
        )),
        _ => {
            tracing::error!(
                "this database holds more than one site and this build serves one: \
                 nothing here can say which site a request is about"
            );

            Err(AppError::Conflict(
                say::THIS_DATABASE_HOLDS_MORE_THAN_ONE_SITE.into(),
            ))
        }
    }
}
