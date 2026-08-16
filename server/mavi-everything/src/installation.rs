//! A single, complete site installation.
//!
//! Owns its own database connection, file storage, keyring (seals), and build provider.
//! Can construct its HTTP router independently with or without background workers, ensuring
//! no global state is shared across installations (Issue #134).

use std::sync::Arc;

use axum::Router;
use mavi_core::ports::{Builds, Files, Seals};
use mavi_db::Db;
use mavi_serve::WhoIsAsking;
use mavi_work::Queue;

/// A self-contained site installation.
#[derive(Clone)]
pub struct Installation {
    pub db: Db,
    pub files: Arc<dyn Files>,
    pub seals: Option<Arc<dyn Seals>>,
    pub builds: Arc<dyn Builds>,
    pub router: Router,
}

impl std::fmt::Debug for Installation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Installation")
            .field("has_seals", &self.seals.is_some())
            .finish_non_exhaustive()
    }
}

impl Installation {
    /// Constructs a new site installation and mounts its complete router.
    #[must_use]
    pub fn new(
        db: Db,
        files: Arc<dyn Files>,
        seals: Option<Arc<dyn Seals>>,
        builds: Arc<dyn Builds>,
        who_is_asking: WhoIsAsking,
    ) -> Self {
        let router = crate::mounted::with_all_of_it(&db, &files, &seals, who_is_asking);
        Self {
            db,
            files,
            seals,
            builds,
            router,
        }
    }

    /// Returns a clone of this installation's HTTP router.
    pub fn router(&self) -> Router {
        self.router.clone()
    }

    /// Returns a reference to this installation's database handle.
    #[must_use]
    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Returns a queue instance configured for all work kinds this installation runs.
    #[must_use]
    pub fn queue(&self) -> Queue {
        Queue::of(&crate::work())
    }
}
