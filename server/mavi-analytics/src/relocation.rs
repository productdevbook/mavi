use std::collections::BTreeSet;

use chrono::{DateTime, NaiveDate, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_core::{MaviError, Result, SiteContext, SiteId};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use super::{AnalyticsService, MAX_EVENT_NAME, MAX_PATH, MAX_VALUE};

pub const ANALYTICS_RELOCATION_FORMAT: &str = "mavi.analytics.relocation";
pub const ANALYTICS_RELOCATION_VERSION: u16 = 1;
pub const MAX_ANALYTICS_RELOCATION_RECORDS: usize = 500_000;
pub const MAX_ANALYTICS_RELOCATION_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyticsRelocation {
    pub format: String,
    pub version: u16,
    pub source_site_id: SiteId,
    pub events: Vec<AnalyticsEventRelocation>,
    pub daily: Vec<AnalyticsDailyRelocation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyticsEventRelocation {
    pub id: Uuid,
    pub event_name: String,
    pub path: String,
    pub value: i64,
    pub occurred_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyticsDailyRelocation {
    pub day: NaiveDate,
    pub event_name: String,
    pub path: String,
    pub event_count: i64,
    pub value_sum: i64,
    pub value_min: i64,
    pub value_max: i64,
}

impl AnalyticsRelocation {
    #[must_use]
    pub fn empty(source_site_id: SiteId) -> Self {
        Self {
            format: ANALYTICS_RELOCATION_FORMAT.to_owned(),
            version: ANALYTICS_RELOCATION_VERSION,
            source_site_id,
            events: Vec::new(),
            daily: Vec::new(),
        }
    }

    pub fn validate_for_relocation(&self, target_site: SiteId) -> Result<()> {
        if self.format != ANALYTICS_RELOCATION_FORMAT {
            return Err(MaviError::validation("analytics_relocation_format_invalid"));
        }
        if self.version != ANALYTICS_RELOCATION_VERSION {
            return Err(MaviError::validation(
                "analytics_relocation_version_unsupported",
            ));
        }
        if self.source_site_id != target_site || self.source_site_id.into_uuid().is_nil() {
            return Err(MaviError::conflict("analytics_relocation_site_mismatch"));
        }
        let total = self
            .events
            .len()
            .checked_add(self.daily.len())
            .ok_or_else(|| MaviError::validation("analytics_relocation_count_overflow"))?;
        if total > MAX_ANALYTICS_RELOCATION_RECORDS
            || self.events.len() > MAX_ANALYTICS_RELOCATION_RECORDS
            || self.daily.len() > MAX_ANALYTICS_RELOCATION_RECORDS
        {
            return Err(MaviError::validation("analytics_relocation_counts_invalid"));
        }
        let mut event_ids = BTreeSet::new();
        for event in &self.events {
            if event.id.is_nil()
                || !event_ids.insert(event.id)
                || !valid_event_name(&event.event_name)
                || !valid_path(&event.path)
                || !(0..=MAX_VALUE).contains(&event.value)
            {
                return Err(MaviError::validation("analytics_relocation_event_invalid"));
            }
        }
        let mut daily_keys = BTreeSet::new();
        for aggregate in &self.daily {
            if !valid_event_name(&aggregate.event_name)
                || !valid_path(&aggregate.path)
                || aggregate.event_count <= 0
                || aggregate.value_sum < 0
                || !(0..=MAX_VALUE).contains(&aggregate.value_min)
                || !(0..=MAX_VALUE).contains(&aggregate.value_max)
                || aggregate.value_min > aggregate.value_max
                || !daily_keys.insert((
                    aggregate.day,
                    aggregate.event_name.clone(),
                    aggregate.path.clone(),
                ))
            {
                return Err(MaviError::validation("analytics_relocation_daily_invalid"));
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| MaviError::Internal)?
            .len()
            > MAX_ANALYTICS_RELOCATION_BYTES
        {
            return Err(MaviError::validation("analytics_relocation_too_large"));
        }
        Ok(())
    }

    pub fn record_count(&self) -> Result<i64> {
        let count = self
            .events
            .len()
            .checked_add(self.daily.len())
            .ok_or_else(|| MaviError::validation("analytics_relocation_count_overflow"))?;
        i64::try_from(count)
            .map_err(|_| MaviError::validation("analytics_relocation_count_overflow"))
    }
}

impl AnalyticsService {
    pub async fn export_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
    ) -> Result<AnalyticsRelocation> {
        let site_id = context.site_id.into_uuid();
        let events = sqlx::query(
            "select id, event_name, path, value, occurred_at, created_at
               from analytics_events where site_id = $1 order by occurred_at, id",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(AnalyticsEventRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                event_name: row.try_get("event_name").map_err(|_| MaviError::Internal)?,
                path: row.try_get("path").map_err(|_| MaviError::Internal)?,
                value: row.try_get("value").map_err(|_| MaviError::Internal)?,
                occurred_at: row
                    .try_get("occurred_at")
                    .map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let daily = sqlx::query(
            "select day, event_name, path, event_count, value_sum, value_min, value_max
               from analytics_daily where site_id = $1 order by day, event_name, path",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(AnalyticsDailyRelocation {
                day: row.try_get("day").map_err(|_| MaviError::Internal)?,
                event_name: row.try_get("event_name").map_err(|_| MaviError::Internal)?,
                path: row.try_get("path").map_err(|_| MaviError::Internal)?,
                event_count: row
                    .try_get("event_count")
                    .map_err(|_| MaviError::Internal)?,
                value_sum: row.try_get("value_sum").map_err(|_| MaviError::Internal)?,
                value_min: row.try_get("value_min").map_err(|_| MaviError::Internal)?,
                value_max: row.try_get("value_max").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let relocation = AnalyticsRelocation {
            format: ANALYTICS_RELOCATION_FORMAT.to_owned(),
            version: ANALYTICS_RELOCATION_VERSION,
            source_site_id: context.site_id,
            events,
            daily,
        };
        relocation.validate_for_relocation(context.site_id)?;
        Ok(relocation)
    }

    pub async fn import_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        relocation: &AnalyticsRelocation,
    ) -> Result<()> {
        relocation.validate_for_relocation(context.site_id)?;
        let site_id = context.site_id.into_uuid();
        sqlx::query("delete from analytics_events where site_id = $1")
            .bind(site_id)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        sqlx::query("delete from analytics_daily where site_id = $1")
            .bind(site_id)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        for event in &relocation.events {
            sqlx::query(
                "insert into analytics_events
                    (site_id, id, event_name, path, value, occurred_at, created_at)
                 values ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(site_id)
            .bind(event.id)
            .bind(&event.event_name)
            .bind(&event.path)
            .bind(event.value)
            .bind(event.occurred_at)
            .bind(event.created_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        for aggregate in &relocation.daily {
            sqlx::query(
                "insert into analytics_daily
                    (site_id, day, event_name, path, event_count, value_sum, value_min, value_max)
                 values ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(site_id)
            .bind(aggregate.day)
            .bind(&aggregate.event_name)
            .bind(&aggregate.path)
            .bind(aggregate.event_count)
            .bind(aggregate.value_sum)
            .bind(aggregate.value_min)
            .bind(aggregate.value_max)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "portable.analytics.relocated".to_owned(),
                    resource_type: "AnalyticsSnapshot".to_owned(),
                    resource_id: None,
                    payload: serde_json::json!({
                        "events": relocation.events.len(),
                        "daily": relocation.daily.len(),
                        "raw_event_identity_preserved": true,
                    }),
                },
            )
            .await
    }
}

fn valid_event_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_EVENT_NAME
        && !value.chars().any(char::is_control)
}

fn valid_path(value: &str) -> bool {
    value.starts_with('/')
        && value.chars().count() <= MAX_PATH
        && !value.contains('?')
        && !value.contains('#')
        && !value.chars().any(char::is_control)
}
