//! Multi-site hosting coordinator (Issue #134).
//!
//! Enables serving multiple independent site installations in a single process,
//! routing requests by `Host` header via `HostDispatcher` and managing background workers.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use mavi_serve::HostDispatcher;

use crate::installation::Installation;

/// Holds and coordinates multiple independent site installations in one process.
#[derive(Clone, Default)]
pub struct MultiSite {
    sites: Arc<HashMap<String, Installation>>,
    fallback: Option<Installation>,
}

impl std::fmt::Debug for MultiSite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiSite")
            .field("domains", &self.sites.keys().collect::<Vec<_>>())
            .field("has_fallback", &self.fallback.is_some())
            .finish()
    }
}

impl MultiSite {
    /// Creates an empty multi-site coordinator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sites: Arc::new(HashMap::new()),
            fallback: None,
        }
    }

    /// Registers an installation for a specific domain / hostname.
    #[must_use]
    pub fn with_site(mut self, host: impl Into<String>, site: Installation) -> Self {
        let host = host.into().trim().to_lowercase();
        let mut map = (*self.sites).clone();
        map.insert(host, site);
        self.sites = Arc::new(map);
        self
    }

    /// Sets the default / fallback installation.
    #[must_use]
    pub fn with_fallback(mut self, site: Installation) -> Self {
        self.fallback = Some(site);
        self
    }

    /// Builds a single `HostDispatcher` router that dispatches incoming requests to the
    /// appropriate site installation based on the `Host` header.
    pub fn into_router(self) -> Router {
        let mut dispatcher = HostDispatcher::new();

        for (host, site) in self.sites.iter() {
            dispatcher = dispatcher.with_site(host.clone(), site.router());
        }

        if let Some(fallback) = self.fallback {
            dispatcher = dispatcher.with_fallback(fallback.router());
        }

        dispatcher.into_router()
    }

    /// Returns an iterator over all registered site installations.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Installation)> {
        self.sites.iter()
    }

    /// Returns the number of registered sites.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sites.len()
    }

    /// Returns true if no sites are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }
}
