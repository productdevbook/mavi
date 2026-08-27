//! Application boundary for portable export/import.
//!
//! `mavi-portable` owns the versioned bundle format and its storage adapter.
//! The HTTP layer depends on this application façade so transaction and
//! authorization orchestration has one home and the wire format does not leak
//! into domain composition.

use mavi_core::{Result, SiteContext};
use mavi_storage::SiteTx;

pub use mavi_portable::{ImportReceipt, PortableBundle, PortableImportRequest};

#[derive(Clone, Copy, Debug)]
pub struct PortableService {
    storage: mavi_portable::PortableService,
}

impl Default for PortableService {
    fn default() -> Self {
        Self::new()
    }
}

impl PortableService {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            storage: mavi_portable::PortableService,
        }
    }

    pub async fn export(&self, tx: &mut SiteTx, context: &SiteContext) -> Result<PortableBundle> {
        self.storage.export(tx, context).await
    }

    pub async fn import(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        request: &PortableImportRequest,
    ) -> Result<ImportReceipt> {
        self.storage.import(tx, context, request).await
    }
}
