//! Process-level worker pool (Issue #134).
//!
//! Coordinates background job execution across one or many site installations in a single process.
//! Enforces process-wide concurrency limits (e.g., maximum simultaneous build jobs) to avoid
//! saturating system resources, while ensuring fair execution across multiple sites.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

/// Configuration for the process worker pool.
#[derive(Clone, Debug)]
pub struct PoolConfig {
    /// Worker identifier name.
    pub name: String,
    /// Delay when no work is found in the queue.
    pub pause_when_empty: Duration,
    /// Maximum simultaneous build/heavy jobs allowed across the whole process.
    pub max_concurrent_builds: usize,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            name: "mavi-worker".to_owned(),
            pause_when_empty: Duration::from_millis(500),
            max_concurrent_builds: 4,
        }
    }
}

/// Process-wide resource guards shared across all installations.
#[derive(Clone, Debug)]
pub struct ProcessLimits {
    /// Semaphore limiting heavy operations (like static builds or transcoding) across all sites.
    pub build_semaphore: Arc<Semaphore>,
}

impl ProcessLimits {
    /// Creates limits based on the given pool configuration.
    #[must_use]
    pub fn new(config: &PoolConfig) -> Self {
        Self {
            build_semaphore: Arc::new(Semaphore::new(config.max_concurrent_builds.max(1))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_limits_initializes_semaphore() {
        let config = PoolConfig {
            name: "test-worker".to_owned(),
            pause_when_empty: Duration::from_millis(100),
            max_concurrent_builds: 3,
        };
        let limits = ProcessLimits::new(&config);
        assert_eq!(limits.build_semaphore.available_permits(), 3);
    }
}
