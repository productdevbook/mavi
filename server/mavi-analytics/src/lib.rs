//! Privacy-preserving, site-scoped analytics.
//!
//! Analytics intentionally has a small event contract. It stores an event
//! name, a route path and an optional non-negative numeric value; there is no
//! visitor fingerprint, IP address, user-agent or arbitrary properties bag.
//! Raw events are useful for a bounded export window, while the daily table is
//! the stable reporting surface and can be retained independently.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, NaiveDate, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, AnalyticsEventId, AnalyticsRetentionPolicy, Capability, Cursor, ErrorCode, JobId,
    MaviError, Page, PageRequest, Result, SiteContext,
};
use mavi_jobs::{JobKind, JobsService};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Postgres, QueryBuilder, Row};
use uuid::Uuid;

mod relocation;

pub use relocation::{AnalyticsDailyRelocation, AnalyticsEventRelocation, AnalyticsRelocation};

pub const MAX_BATCH: usize = 100;
pub const MAX_EVENT_NAME: usize = 120;
pub const MAX_PATH: usize = 500;
pub const MAX_VALUE: i64 = 9_000_000_000_000_000;
pub const MAX_RETENTION_DAYS: u16 = 3_650;
pub const ANALYTICS_RETENTION_JOB: JobKind = JobKind::new("analytics.retention", 5);
pub const ANALYTICS_RETENTION_BUCKET_SECONDS: i64 = 24 * 60 * 60;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyticsEventInput {
    pub event_name: String,
    pub path: String,
    pub value: Option<i64>,
    pub occurred_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyticsEventBatch {
    pub events: Vec<AnalyticsEventInput>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AnalyticsReceipt {
    pub accepted: u32,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub event_name: Option<String>,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AnalyticsEvent {
    pub id: AnalyticsEventId,
    pub event_name: String,
    pub path: String,
    pub value: i64,
    pub occurred_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DailyListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub event_name: Option<String>,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DailyAggregate {
    pub day: NaiveDate,
    pub event_name: String,
    pub path: String,
    pub event_count: i64,
    pub value_sum: i64,
    pub value_min: i64,
    pub value_max: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PruneAnalytics {
    pub raw_days: u16,
    pub aggregate_days: u16,
}

#[derive(Clone, Debug, Serialize)]
pub struct PruneReceipt {
    pub deleted_events: u64,
    pub deleted_aggregates: u64,
}

/// Payload for the idempotent daily analytics retention job.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyticsRetentionJob {
    pub bucket: i64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AnalyticsService;

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn api() -> mavi_contract::Api {
    let view = Permission {
        capability: Capability::Analytics,
        action: Action::View,
    };
    let delete = Permission {
        capability: Capability::Analytics,
        action: Action::Delete,
    };
    mavi_contract::Api::new(vec![
        Endpoint::new(
            Method::Post,
            "/public/v1/analytics/events",
            "analytics.events.ingest",
            "Record bounded privacy-preserving analytics events",
        )
        .public_changes(false)
        .takes("AnalyticsEventBatch")
        .returns(202, "AnalyticsReceipt")
        .refuses([
            ErrorCode::Validation,
            ErrorCode::RateLimited,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/analytics/events",
            "analytics.events.list",
            "Export recent raw analytics events",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("EventListFilter")
        .returns(200, "AnalyticsEventPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/analytics/daily",
            "analytics.daily.list",
            "List daily analytics aggregates",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("DailyListFilter")
        .returns(200, "DailyAggregatePage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/analytics/prune",
            "analytics.retention.prune",
            "Delete raw and aggregate analytics data outside retention windows",
        )
        .account_or_assistant()
        .requires(delete)
        .takes("PruneAnalytics")
        .returns(200, "PruneReceipt")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
    ])
    .with_shapes(shapes())
}

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "AnalyticsEventInput",
            json!({
                "type": "object",
                "required": ["event_name", "path", "value", "occurred_at"],
                "properties": {
                    "event_name": {"type":"string","minLength":1,"maxLength":120},
                    "path": {"type":"string","minLength":1,"maxLength":500},
                    "value": {"type":["integer","null"],"minimum":0},
                    "occurred_at": {"type":["string","null"],"format":"date-time"}
                }
            }),
        ),
        Shape::new(
            "AnalyticsEventBatch",
            json!({
                "type":"object",
                "required":["events"],
                "properties":{"events":{"type":"array","minItems":1,"maxItems":100,"items":{"$ref":"#/components/schemas/AnalyticsEventInput"}}}
            }),
        ),
        Shape::new(
            "AnalyticsReceipt",
            json!({"type":"object","required":["accepted"],"properties":{"accepted":{"type":"integer","minimum":0}}}),
        ),
        Shape::new(
            "EventListFilter",
            json!({"type":"object","properties":{"after":{"type":["string","null"],"maxLength":512},"limit":{"type":"integer","minimum":1,"maximum":100},"event_name":{"type":["string","null"],"maxLength":120},"path":{"type":["string","null"],"maxLength":500}}}),
        ),
        Shape::new(
            "AnalyticsEvent",
            json!({
                "type":"object",
                "required":["id","event_name","path","value","occurred_at","created_at"],
                "properties":{"id":{"type":"string","format":"uuid"},"event_name":{"type":"string"},"path":{"type":"string"},"value":{"type":"integer"},"occurred_at":{"type":"string","format":"date-time"},"created_at":{"type":"string","format":"date-time"}}
            }),
        ),
        Shape::new(
            "AnalyticsEventPage",
            json!({"type":"object","required":["items","next_cursor"],"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/AnalyticsEvent"}},"next_cursor":{"type":["string","null"]}}}),
        ),
        Shape::new(
            "DailyListFilter",
            json!({"type":"object","properties":{"after":{"type":["string","null"],"maxLength":512},"limit":{"type":"integer","minimum":1,"maximum":100},"event_name":{"type":["string","null"],"maxLength":120},"path":{"type":["string","null"],"maxLength":500}}}),
        ),
        Shape::new(
            "DailyAggregate",
            json!({
                "type":"object",
                "required":["day","event_name","path","event_count","value_sum","value_min","value_max"],
                "properties":{"day":{"type":"string","format":"date"},"event_name":{"type":"string"},"path":{"type":"string"},"event_count":{"type":"integer"},"value_sum":{"type":"integer"},"value_min":{"type":"integer"},"value_max":{"type":"integer"}}
            }),
        ),
        Shape::new(
            "DailyAggregatePage",
            json!({"type":"object","required":["items","next_cursor"],"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/DailyAggregate"}},"next_cursor":{"type":["string","null"]}}}),
        ),
        Shape::new(
            "PruneAnalytics",
            json!({"type":"object","required":["raw_days","aggregate_days"],"properties":{"raw_days":{"type":"integer","minimum":1,"maximum":3650},"aggregate_days":{"type":"integer","minimum":1,"maximum":3650}}}),
        ),
        Shape::new(
            "PruneReceipt",
            json!({"type":"object","required":["deleted_events","deleted_aggregates"],"properties":{"deleted_events":{"type":"integer","minimum":0},"deleted_aggregates":{"type":"integer","minimum":0}}}),
        ),
    ]
}

impl AnalyticsService {
    /// Enqueues one retention pass per UTC day for the current site.
    pub async fn enqueue_retention_job(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        jobs: &JobsService,
        now: DateTime<Utc>,
    ) -> Result<JobId> {
        let bucket = now
            .timestamp()
            .div_euclid(ANALYTICS_RETENTION_BUCKET_SECONDS);
        let payload = serde_json::to_value(AnalyticsRetentionJob { bucket })
            .map_err(|_| MaviError::Internal)?;
        let idempotency_key = format!("analytics:retention:{}:{}", context.site_id, bucket);
        jobs.enqueue(
            tx,
            context,
            ANALYTICS_RETENTION_JOB.name,
            &payload,
            None,
            Some(&idempotency_key),
        )
        .await
    }

    /// Records events and updates the daily roll-up in the caller's existing
    /// transaction. The endpoint intentionally does not write an audit row for
    /// every telemetry event; retention changes are audited separately.
    pub async fn record_batch(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &AnalyticsEventBatch,
    ) -> Result<AnalyticsReceipt> {
        if input.events.is_empty() || input.events.len() > MAX_BATCH {
            return Err(MaviError::validation("analytics_batch_invalid"));
        }

        for event in &input.events {
            let event_name = bounded_text(
                &event.event_name,
                MAX_EVENT_NAME,
                "analytics_event_name_invalid",
            )?;
            let path = normalize_path(&event.path)?;
            let value = event.value.unwrap_or(0);
            if !(0..=MAX_VALUE).contains(&value) {
                return Err(MaviError::validation("analytics_value_invalid"));
            }
            let occurred_at = event.occurred_at.unwrap_or_else(Utc::now);
            if occurred_at > Utc::now() + chrono::Duration::hours(24) {
                return Err(MaviError::validation("analytics_occurred_at_invalid"));
            }

            let id = AnalyticsEventId::new();
            sqlx::query(
                "insert into analytics_events
                    (site_id, id, event_name, path, value, occurred_at)
                 values ($1, $2, $3, $4, $5, $6)",
            )
            .bind(context.site_id.into_uuid())
            .bind(id.into_uuid())
            .bind(&event_name)
            .bind(&path)
            .bind(value)
            .bind(occurred_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;

            sqlx::query(
                "insert into analytics_daily
                    (site_id, day, event_name, path, event_count, value_sum, value_min, value_max)
                 values ($1, $2, $3, $4, 1, $5, $5, $5)
                 on conflict (site_id, day, event_name, path)
                 do update set
                    event_count = analytics_daily.event_count + 1,
                    value_sum = analytics_daily.value_sum + excluded.value_sum,
                    value_min = least(analytics_daily.value_min, excluded.value_min),
                    value_max = greatest(analytics_daily.value_max, excluded.value_max)",
            )
            .bind(context.site_id.into_uuid())
            .bind(occurred_at.date_naive())
            .bind(&event_name)
            .bind(&path)
            .bind(value)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }

        Ok(AnalyticsReceipt {
            accepted: u32::try_from(input.events.len()).map_err(|_| MaviError::Internal)?,
        })
    }

    pub async fn list_events(
        &self,
        tx: &mut SiteTx,
        filter: &EventListFilter,
    ) -> Result<Page<AnalyticsEvent>> {
        let after = filter
            .page
            .after
            .as_ref()
            .map(decode_event_cursor)
            .transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = QueryBuilder::<Postgres>::new(
            "select id, event_name, path, value, occurred_at, created_at
             from analytics_events where true",
        );
        if let Some(event_name) = &filter.event_name {
            query.push(" and event_name = ").push_bind(bounded_text(
                event_name,
                MAX_EVENT_NAME,
                "analytics_event_name_invalid",
            )?);
        }
        if let Some(path) = &filter.path {
            query.push(" and path = ").push_bind(normalize_path(path)?);
        }
        if let Some(after) = after {
            query
                .push(" and (occurred_at, id) < (")
                .push_bind(after.occurred_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        query
            .push(" order by occurred_at desc, id desc limit ")
            .push_bind(limit + 1);
        let rows = query
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows.iter().map(event_row).collect::<Result<Vec<_>>>()?;
        let page_limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > page_limit {
            let item = items
                .get(page_limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_event_cursor(item.occurred_at, item.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(page_limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn list_daily(
        &self,
        tx: &mut SiteTx,
        filter: &DailyListFilter,
    ) -> Result<Page<DailyAggregate>> {
        let after = filter
            .page
            .after
            .as_ref()
            .map(decode_daily_cursor)
            .transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = QueryBuilder::<Postgres>::new(
            "select day, event_name, path, event_count, value_sum, value_min, value_max
             from analytics_daily where true",
        );
        if let Some(event_name) = &filter.event_name {
            query.push(" and event_name = ").push_bind(bounded_text(
                event_name,
                MAX_EVENT_NAME,
                "analytics_event_name_invalid",
            )?);
        }
        if let Some(path) = &filter.path {
            query.push(" and path = ").push_bind(normalize_path(path)?);
        }
        if let Some(after) = after {
            query
                .push(" and (day < ")
                .push_bind(after.day)
                .push(" or (day = ")
                .push_bind(after.day)
                .push(" and (event_name > ")
                .push_bind(&after.event_name)
                .push(" or (event_name = ")
                .push_bind(&after.event_name)
                .push(" and path > ")
                .push_bind(&after.path)
                .push(")))");
        }
        query
            .push(" order by day desc, event_name asc, path asc limit ")
            .push_bind(limit + 1);
        let rows = query
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows.iter().map(daily_row).collect::<Result<Vec<_>>>()?;
        let page_limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > page_limit {
            let item = items
                .get(page_limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_daily_cursor(item.day, &item.event_name, &item.path)?)
        } else {
            None
        };
        items.truncate(page_limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn prune(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &PruneAnalytics,
    ) -> Result<PruneReceipt> {
        validate_retention(input.raw_days)?;
        validate_retention(input.aggregate_days)?;
        self.prune_inner(tx, context, input, "analytics.retention.pruned", None)
            .await
    }

    pub async fn prune_scheduled(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        policy: AnalyticsRetentionPolicy,
        bucket: i64,
    ) -> Result<PruneReceipt> {
        if bucket < 0 {
            return Err(MaviError::validation("analytics_retention_bucket_invalid"));
        }
        let input = PruneAnalytics {
            raw_days: policy.raw_days,
            aggregate_days: policy.aggregate_days,
        };
        self.prune_inner(
            tx,
            context,
            &input,
            "analytics.retention.scheduled_pruned",
            Some(bucket),
        )
        .await
    }

    async fn prune_inner(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &PruneAnalytics,
        audit_action: &str,
        bucket: Option<i64>,
    ) -> Result<PruneReceipt> {
        let deleted_events = sqlx::query(
            "delete from analytics_events
             where created_at < now() - ($1::bigint * interval '1 day')",
        )
        .bind(i64::from(input.raw_days))
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .rows_affected();
        let deleted_aggregates = sqlx::query(
            "delete from analytics_daily
             where day < current_date - $1::integer",
        )
        .bind(i32::from(input.aggregate_days))
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .rows_affected();

        let receipt = PruneReceipt {
            deleted_events,
            deleted_aggregates,
        };
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: audit_action.to_owned(),
                    resource_type: "AnalyticsRetention".to_owned(),
                    resource_id: None,
                    payload: json!({
                        "raw_days": input.raw_days,
                        "aggregate_days": input.aggregate_days,
                        "deleted_events": receipt.deleted_events,
                        "deleted_aggregates": receipt.deleted_aggregates,
                        "bucket": bucket
                    }),
                },
            )
            .await?;
        Ok(receipt)
    }
}

fn bounded_text(value: &str, max: usize, code: &'static str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(MaviError::validation(code));
    }
    Ok(value.to_owned())
}

fn normalize_path(value: &str) -> Result<String> {
    let path = bounded_text(value, MAX_PATH, "analytics_path_invalid")?;
    if !path.starts_with('/') || path.contains('?') || path.contains('#') {
        return Err(MaviError::validation("analytics_path_invalid"));
    }
    Ok(path)
}

fn validate_retention(days: u16) -> Result<()> {
    if !(1..=MAX_RETENTION_DAYS).contains(&days) {
        return Err(MaviError::validation("analytics_retention_invalid"));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct EventCursor {
    occurred_at: DateTime<Utc>,
    id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DailyCursor {
    day: NaiveDate,
    event_name: String,
    path: String,
}

fn encode_event_cursor(occurred_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes =
        serde_json::to_vec(&EventCursor { occurred_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_event_cursor(cursor: &Cursor) -> Result<EventCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

fn encode_daily_cursor(day: NaiveDate, event_name: &str, path: &str) -> Result<Cursor> {
    let bytes = serde_json::to_vec(&DailyCursor {
        day,
        event_name: event_name.to_owned(),
        path: path.to_owned(),
    })
    .map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_daily_cursor(cursor: &Cursor) -> Result<DailyCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

fn event_row(row: &sqlx::postgres::PgRow) -> Result<AnalyticsEvent> {
    Ok(AnalyticsEvent {
        id: AnalyticsEventId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        event_name: row.try_get("event_name").map_err(|_| MaviError::Internal)?,
        path: row.try_get("path").map_err(|_| MaviError::Internal)?,
        value: row.try_get("value").map_err(|_| MaviError::Internal)?,
        occurred_at: row
            .try_get("occurred_at")
            .map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
    })
}

fn daily_row(row: &sqlx::postgres::PgRow) -> Result<DailyAggregate> {
    Ok(DailyAggregate {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_cannot_carry_query_data() {
        assert!(normalize_path("/docs").is_ok());
        assert!(normalize_path("/docs?email=person@example.com").is_err());
        assert!(normalize_path("docs").is_err());
    }

    #[test]
    fn analytics_contracts_are_cursor_only() {
        let api = api();
        for name in ["EventListFilter", "DailyListFilter"] {
            let shape = shapes()
                .into_iter()
                .find(|shape| shape.name == name)
                .expect("filter shape");
            let properties = shape.schema["properties"].as_object().expect("properties");
            assert!(properties.contains_key("after"));
            assert!(properties.contains_key("limit"));
            assert!(!properties.contains_key("offset"));
            assert!(!properties.contains_key("page"));
        }
        api.validate().expect("analytics API");
    }
}
