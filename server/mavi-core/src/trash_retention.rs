use serde::{Deserialize, Serialize};

use crate::{MaviError, Result};

pub const DEFAULT_TRASH_RETENTION_DAYS: u16 = 30;
pub const MAX_TRASH_RETENTION_DAYS: u16 = 3_650;

/// Site policy for how long soft-deleted rows remain restorable.
///
/// The policy is deliberately bounded and deployment-neutral. The worker
/// applies it to the site's scoped transaction; no domain needs to know how
/// the setting was configured or where the bytes are stored.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrashRetentionPolicy {
    pub days: u16,
}

impl TrashRetentionPolicy {
    pub fn new(days: u16) -> Result<Self> {
        if !(1..=MAX_TRASH_RETENTION_DAYS).contains(&days) {
            return Err(MaviError::validation("trash_retention_invalid"));
        }
        Ok(Self { days })
    }
}

impl Default for TrashRetentionPolicy {
    fn default() -> Self {
        Self {
            days: DEFAULT_TRASH_RETENTION_DAYS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_policy_is_bounded_and_defaults_are_explicit() {
        assert_eq!(TrashRetentionPolicy::default().days, 30);
        assert!(TrashRetentionPolicy::new(1).is_ok());
        assert!(TrashRetentionPolicy::new(3_650).is_ok());
        assert!(TrashRetentionPolicy::new(0).is_err());
        assert!(TrashRetentionPolicy::new(3_651).is_err());
    }
}
