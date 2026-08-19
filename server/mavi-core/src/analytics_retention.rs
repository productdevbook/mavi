use serde::{Deserialize, Serialize};

use crate::{MaviError, Result};

pub const DEFAULT_ANALYTICS_RAW_RETENTION_DAYS: u16 = 365;
pub const DEFAULT_ANALYTICS_AGGREGATE_RETENTION_DAYS: u16 = 3_650;
pub const MAX_ANALYTICS_RETENTION_DAYS: u16 = 3_650;

/// Site policy for raw analytics and its derived daily aggregates.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AnalyticsRetentionPolicy {
    pub raw_days: u16,
    pub aggregate_days: u16,
}

impl AnalyticsRetentionPolicy {
    pub fn new(raw_days: u16, aggregate_days: u16) -> Result<Self> {
        if !(1..=MAX_ANALYTICS_RETENTION_DAYS).contains(&raw_days)
            || !(1..=MAX_ANALYTICS_RETENTION_DAYS).contains(&aggregate_days)
        {
            return Err(MaviError::validation("analytics_retention_invalid"));
        }
        Ok(Self {
            raw_days,
            aggregate_days,
        })
    }
}

impl Default for AnalyticsRetentionPolicy {
    fn default() -> Self {
        Self {
            raw_days: DEFAULT_ANALYTICS_RAW_RETENTION_DAYS,
            aggregate_days: DEFAULT_ANALYTICS_AGGREGATE_RETENTION_DAYS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_policy_is_bounded_and_defaults_are_explicit() {
        assert_eq!(AnalyticsRetentionPolicy::default().raw_days, 365);
        assert!(AnalyticsRetentionPolicy::new(1, 3_650).is_ok());
        assert!(AnalyticsRetentionPolicy::new(0, 30).is_err());
        assert!(AnalyticsRetentionPolicy::new(30, 3_651).is_err());
    }
}
