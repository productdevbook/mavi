//! Fixed single-site runtime composition.
//!
//! Mavi is deployed with one site ID, one database and one file namespace.
//! Tenant routing, placement and lifecycle belong to an external control
//! plane; this crate only carries the immutable site boundary into requests.

use mavi_core::{Caller, MaviError, PluginId, RequestId, Result, SiteContext, SiteId};
use mavi_storage::{CURRENT_SCHEMA_VERSION, Database, SiteTx};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const RUNTIME_PROTOCOL: &str = "mavi.runtime.v1";
pub const API_CONTRACT_VERSION: &str = "v1";
pub const PAGINATION_STYLE: &str = "cursor";
pub const MAX_PAGE_LIMIT: u16 = 100;

/// The machine-readable compatibility contract consumed by the panel or an
/// operator after an instance is provisioned.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimeManifest {
    pub protocol: String,
    pub release: String,
    pub api_contract_version: String,
    pub api_contract_hash: String,
    pub storage_schema_version: u32,
    pub site_id: SiteId,
    pub active_plugins: Vec<PluginId>,
    pub pagination: PaginationContract,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PaginationContract {
    pub style: String,
    pub default_limit: u16,
    pub max_limit: u16,
}

impl RuntimeManifest {
    #[must_use]
    pub fn new(
        site_id: SiteId,
        api_contract_hash: String,
        active_plugins: impl IntoIterator<Item = PluginId>,
    ) -> Self {
        Self {
            protocol: RUNTIME_PROTOCOL.to_owned(),
            release: env!("CARGO_PKG_VERSION").to_owned(),
            api_contract_version: API_CONTRACT_VERSION.to_owned(),
            api_contract_hash,
            storage_schema_version: CURRENT_SCHEMA_VERSION,
            site_id,
            active_plugins: active_plugins.into_iter().collect(),
            pagination: PaginationContract {
                style: PAGINATION_STYLE.to_owned(),
                default_limit: mavi_core::PageRequest::DEFAULT_LIMIT,
                max_limit: MAX_PAGE_LIMIT,
            },
        }
    }
}

/// The only runtime object available to the application. Its site ID is
/// immutable for the lifetime of the process, so a request cannot select a
/// different tenant from a host header or arbitrary input.
#[derive(Clone, Debug)]
pub struct SiteRuntime {
    database: Database,
    site_id: SiteId,
}

impl SiteRuntime {
    #[must_use]
    pub const fn new(database: Database, site_id: SiteId) -> Self {
        Self { database, site_id }
    }

    #[must_use]
    pub const fn site_id(&self) -> SiteId {
        self.site_id
    }

    #[must_use]
    pub fn database(&self) -> Database {
        self.database.clone()
    }

    pub fn context(&self, request_id: RequestId) -> Result<SiteContext> {
        Ok(SiteContext::with_caller(
            self.site_id,
            Caller::Public,
            request_id,
        ))
    }

    pub async fn begin(&self, context: &SiteContext) -> Result<SiteTx> {
        if context.site_id != self.site_id {
            return Err(MaviError::Forbidden);
        }
        self.database.begin(context).await
    }

    pub async fn ready(&self) -> Result<()> {
        self.database.health_check().await
    }

    #[must_use]
    pub fn manifest(
        &self,
        api_contract_hash: String,
        active_plugins: impl IntoIterator<Item = PluginId>,
    ) -> RuntimeManifest {
        RuntimeManifest::new(self.site_id, api_contract_hash, active_plugins)
    }
}

/// Compatibility name for embedding code that already called the fixed
/// runtime explicitly. It is not generic and cannot represent a shard.
pub type FixedSiteRuntime = SiteRuntime;

pub fn parse_site_id(value: &str) -> Result<SiteId> {
    Uuid::parse_str(value)
        .map(SiteId::from_uuid)
        .map_err(|_| MaviError::validation("invalid_site_id"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_manifest_is_single_site() {
        let site_id = SiteId::new();
        let manifest = RuntimeManifest::new(
            site_id,
            "sha256:test".to_owned(),
            [PluginId::Core, PluginId::Writing],
        );
        assert_eq!(manifest.site_id, site_id);
        assert_eq!(manifest.active_plugins, [PluginId::Core, PluginId::Writing]);
        assert_eq!(manifest.storage_schema_version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn parse_site_id_rejects_invalid_values() {
        assert!(parse_site_id("not-a-uuid").is_err());
        assert!(parse_site_id(&SiteId::new().to_string()).is_ok());
    }
}
