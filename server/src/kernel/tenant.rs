//! Which site an address is.
//!
//! Every request carries a `Host`, and that is the whole of how one site is
//! told from another: the same program serves all of them. An address nothing
//! matches is turned away rather than handed the machine's own installation.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::db::Db;
use super::error::{AppError, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct TenantId(pub Uuid);

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub tenant: TenantId,
    pub frozen: bool,
}

/// An address nothing claims is refused, rather than answered by whichever site
/// is first or by the operator's own installation.
/// The form every address is compared in: no port, no trailing dot, lowercase.
#[must_use]
pub fn normalize_host(host: &str) -> String {
    host.split(':')
        .next()
        .unwrap_or(host)
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

pub async fn resolve_host(db: &Db, host: &str) -> Result<Site> {
    let host = normalize_host(host);

    // Every site's addresses, said out loud: at this point nobody is a site
    // yet, which is the whole reason this read exists.
    let mut conn = db.operator().await?;
    conn.across_sites().await?;

    let found: Option<(Uuid, bool)> = sqlx::query_as(
        "select t.id, t.state = 'suspended'
           from tenant_domains d
           join tenants t on t.id = d.tenant_id
          where d.host = $1
            and t.state in ('live', 'suspended')",
    )
    .bind(&host)
    .fetch_optional(conn.conn())
    .await?;

    conn.commit().await?;

    found
        .map(|(id, frozen)| Site {
            tenant: TenantId(id),
            frozen,
        })
        .ok_or(AppError::UnknownHost)
}
