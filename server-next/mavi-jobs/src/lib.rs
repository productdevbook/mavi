//! Durable, site-scoped work.
//!
//! A job is claimed in a short transaction, executed outside that transaction,
//! and finished with the worker identity and an unexpired lease. This keeps a
//! slow provider from holding a database connection while also making a stale
//! worker unable to overwrite a newer claim. Job kinds are registered by code;
//! accepting an unknown kind would create work no process can ever execute.

use std::{collections::BTreeMap, sync::Arc};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Cursor, ErrorCode, JobId, MaviError, Page, PageRequest, Result, SiteContext,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, QueryBuilder, Row};
use uuid::Uuid;

pub const DEFAULT_LEASE_SECONDS: i64 = 300;
pub const MAX_WORKER_NAME: usize = 160;
pub const MAX_KIND_NAME: usize = 120;
pub const MAX_IDEMPOTENCY_KEY: usize = 160;
pub const MAX_ERROR: usize = 4000;

/// A registered kind of durable work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobKind {
    pub name: &'static str,
    pub max_attempts: u16,
}

impl JobKind {
    #[must_use]
    pub const fn new(name: &'static str, max_attempts: u16) -> Self {
        Self { name, max_attempts }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Ready,
    Running,
    Done,
    Dead,
}

impl JobState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Done => "done",
            Self::Dead => "dead",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "ready" => Ok(Self::Ready),
            "running" => Ok(Self::Running),
            "done" => Ok(Self::Done),
            "dead" => Ok(Self::Dead),
            _ => Err(MaviError::Internal),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub state: Option<JobState>,
    pub kind: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Job {
    pub id: JobId,
    pub kind: String,
    pub payload: Value,
    pub state: JobState,
    pub run_at: DateTime<Utc>,
    pub claimed_until: Option<DateTime<Utc>>,
    pub claimed_by: Option<String>,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub idempotency_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// What a worker receives after claiming a job.
#[derive(Clone, Debug)]
pub struct JobClaim {
    pub id: JobId,
    pub kind: String,
    pub payload: Value,
    pub attempts: i32,
    pub claimed_until: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseOutcome {
    Completed,
    Lost,
}

#[derive(Clone, Debug)]
pub struct JobsService {
    kinds: Arc<BTreeMap<String, u16>>,
}

impl JobsService {
    #[must_use]
    pub fn new(kinds: impl IntoIterator<Item = JobKind>) -> Self {
        let kinds = kinds
            .into_iter()
            .map(|kind| (kind.name.to_owned(), kind.max_attempts.max(1)))
            .collect();
        Self {
            kinds: Arc::new(kinds),
        }
    }

    #[must_use]
    pub fn knows(&self, kind: &str) -> bool {
        self.kinds.contains_key(kind)
    }

    #[must_use]
    pub fn max_attempts(&self, kind: &str) -> Option<u16> {
        self.kinds.get(kind).copied()
    }

    /// Enqueue work in the same transaction as the domain mutation that made
    /// the work necessary. The optional key makes repeated event delivery
    /// return the original job rather than creating another one.
    pub async fn enqueue(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        kind: &str,
        payload: &Value,
        run_at: Option<DateTime<Utc>>,
        idempotency_key: Option<&str>,
    ) -> Result<JobId> {
        self.validate_kind(kind)?;
        validate_payload(payload)?;
        let idempotency_key = idempotency_key.map(normalize_key).transpose()?;

        if let Some(key) = &idempotency_key {
            let existing = sqlx::query(
                "select id, payload from jobs
                   where kind = $1 and idempotency_key = $2",
            )
            .bind(kind)
            .bind(key)
            .fetch_optional(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;

            if let Some(row) = existing {
                let existing_payload: Value =
                    row.try_get("payload").map_err(|_| MaviError::Internal)?;
                if existing_payload != *payload {
                    return Err(MaviError::conflict("job_idempotency_payload_mismatch"));
                }
                let id: Uuid = row.try_get("id").map_err(|_| MaviError::Internal)?;
                return Ok(JobId::from_uuid(id));
            }
        }

        let id = JobId::new();
        let inserted = sqlx::query(
            "insert into jobs
                (site_id, id, kind, payload, run_at, idempotency_key)
             values ($1, $2, $3, $4, coalesce($5, now()), $6)
             on conflict (site_id, kind, idempotency_key)
                 where idempotency_key is not null do nothing
             returning id",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(kind)
        .bind(payload)
        .bind(run_at)
        .bind(idempotency_key.as_deref())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        let id = if let Some(row) = inserted {
            JobId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?)
        } else {
            let key = idempotency_key.as_deref().ok_or(MaviError::Internal)?;
            let row = sqlx::query(
                "select id, payload from jobs where kind = $1 and idempotency_key = $2",
            )
            .bind(kind)
            .bind(key)
            .fetch_one(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            let existing_payload: Value =
                row.try_get("payload").map_err(|_| MaviError::Internal)?;
            if existing_payload != *payload {
                return Err(MaviError::conflict("job_idempotency_payload_mismatch"));
            }
            JobId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?)
        };

        audit(
            tx,
            context,
            "jobs.enqueued",
            "Job",
            id.into_uuid(),
            json!({"kind": kind, "idempotency_key": idempotency_key}),
        )
        .await?;
        Ok(id)
    }

    /// Claim one due job for a worker. The caller commits this transaction
    /// before invoking an external adapter.
    pub async fn claim(
        &self,
        tx: &mut SiteTx,
        worker: &str,
        kinds: &[&str],
        lease_seconds: i64,
    ) -> Result<Option<JobClaim>> {
        validate_worker(worker)?;
        let names = kinds
            .iter()
            .map(|kind| {
                self.validate_kind(kind)?;
                Ok((*kind).to_owned())
            })
            .collect::<Result<Vec<_>>>()?;
        if names.is_empty() {
            return Ok(None);
        }
        let lease_seconds = lease_seconds.clamp(1, 86_400);
        let row = sqlx::query(
            "with candidate as (
                 select site_id, id
                   from jobs
                  where kind = any($1)
                    and ((state = 'ready' and run_at <= now())
                         or (state = 'running' and claimed_until <= now()))
                  order by run_at asc, id asc
                  for update skip locked
                  limit 1
             )
             update jobs as job
                set state = 'running',
                    claimed_until = now() + make_interval(secs => $2),
                    claimed_by = $3,
                    attempts = job.attempts + 1,
                    finished_at = null
               from candidate
              where job.site_id = candidate.site_id and job.id = candidate.id
             returning job.id, job.kind, job.payload, job.attempts, job.claimed_until",
        )
        .bind(names)
        .bind(interval_seconds(lease_seconds))
        .bind(worker)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        row.map(|row| {
            Ok(JobClaim {
                id: JobId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
                kind: row.try_get("kind").map_err(|_| MaviError::Internal)?,
                payload: row.try_get("payload").map_err(|_| MaviError::Internal)?,
                attempts: row.try_get("attempts").map_err(|_| MaviError::Internal)?,
                claimed_until: row
                    .try_get("claimed_until")
                    .map_err(|_| MaviError::Internal)?,
            })
        })
        .transpose()
    }

    pub async fn heartbeat(
        &self,
        tx: &mut SiteTx,
        id: JobId,
        worker: &str,
        lease_seconds: i64,
    ) -> Result<LeaseOutcome> {
        validate_worker(worker)?;
        let rows = sqlx::query(
            "update jobs
                set claimed_until = now() + make_interval(secs => $3)
              where id = $1 and claimed_by = $2 and state = 'running'
                and claimed_until > now()",
        )
        .bind(id.into_uuid())
        .bind(worker)
        .bind(interval_seconds(lease_seconds))
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        Ok(if rows.rows_affected() == 1 {
            LeaseOutcome::Completed
        } else {
            LeaseOutcome::Lost
        })
    }

    pub async fn complete(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: JobId,
        worker: &str,
    ) -> Result<LeaseOutcome> {
        validate_worker(worker)?;
        let rows = sqlx::query(
            "update jobs
                set state = 'done', claimed_until = null, claimed_by = null,
                    finished_at = now()
              where id = $1 and claimed_by = $2 and state = 'running'
                and claimed_until > now()",
        )
        .bind(id.into_uuid())
        .bind(worker)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let outcome = if rows.rows_affected() == 1 {
            audit(
                tx,
                context,
                "jobs.completed",
                "Job",
                id.into_uuid(),
                json!({}),
            )
            .await?;
            LeaseOutcome::Completed
        } else {
            LeaseOutcome::Lost
        };
        Ok(outcome)
    }

    pub async fn fail(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        claim: &JobClaim,
        worker: &str,
        error: &str,
    ) -> Result<LeaseOutcome> {
        validate_worker(worker)?;
        let max_attempts = i32::from(
            self.max_attempts(&claim.kind)
                .ok_or_else(|| MaviError::validation("unknown_job_kind"))?,
        );
        let error = error.chars().take(MAX_ERROR).collect::<String>();
        let delay = retry_delay(claim.attempts);
        let rows = sqlx::query(
            "update jobs
                set state = case when $3 >= $4 then 'dead' else 'ready' end,
                    claimed_until = null, claimed_by = null, last_error = $5,
                    run_at = case when $3 >= $4 then run_at else now() + make_interval(secs => $6) end,
                    finished_at = case when $3 >= $4 then now() else null end
              where id = $1 and claimed_by = $2 and state = 'running'
                and claimed_until > now()",
        )
        .bind(claim.id.into_uuid())
        .bind(worker)
        .bind(claim.attempts)
        .bind(max_attempts)
        .bind(&error)
        .bind(interval_seconds(delay))
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if rows.rows_affected() == 0 {
            return Ok(LeaseOutcome::Lost);
        }
        audit(
            tx,
            context,
            if claim.attempts >= max_attempts {
                "jobs.dead"
            } else {
                "jobs.failed"
            },
            "Job",
            claim.id.into_uuid(),
            json!({"attempts": claim.attempts, "error": error}),
        )
        .await?;
        Ok(LeaseOutcome::Completed)
    }

    pub async fn retry(&self, tx: &mut SiteTx, context: &SiteContext, id: JobId) -> Result<Job> {
        let row = sqlx::query(
            "update jobs
                set state = 'ready', attempts = 0, run_at = now(),
                    claimed_until = null, claimed_by = null, last_error = null,
                    finished_at = null
              where id = $1 and state = 'dead'
             returning id, kind, payload, state, run_at, claimed_until, claimed_by,
                       attempts, last_error, idempotency_key, created_at, finished_at",
        )
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound { resource: "job" })?;
        let job = row_to_job(&row)?;
        audit(
            tx,
            context,
            "jobs.retried",
            "Job",
            id.into_uuid(),
            json!({}),
        )
        .await?;
        Ok(job)
    }

    pub async fn get(&self, tx: &mut SiteTx, id: JobId) -> Result<Job> {
        let row = sqlx::query(
            "select id, kind, payload, state, run_at, claimed_until, claimed_by,
                    attempts, last_error, idempotency_key, created_at, finished_at
               from jobs where id = $1",
        )
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound { resource: "job" })?;
        row_to_job(&row)
    }

    pub async fn list(&self, tx: &mut SiteTx, filter: &JobListFilter) -> Result<Page<Job>> {
        if let Some(kind) = &filter.kind {
            self.validate_kind(kind)?;
        }
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = QueryBuilder::<Postgres>::new(
            "select id, kind, payload, state, run_at, claimed_until, claimed_by,
                    attempts, last_error, idempotency_key, created_at, finished_at
               from jobs where true",
        );
        if let Some(state) = filter.state {
            query.push(" and state = ").push_bind(state.as_str());
        }
        if let Some(kind) = &filter.kind {
            query.push(" and kind = ").push_bind(kind);
        }
        if let Some(after) = after {
            query
                .push(" and (created_at, id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        query
            .push(" order by created_at desc, id desc limit ")
            .push_bind(limit + 1);
        let rows = query
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows.iter().map(row_to_job).collect::<Result<Vec<_>>>()?;
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let last = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_cursor(last.created_at, last.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    fn validate_kind(&self, kind: &str) -> Result<()> {
        if kind.is_empty() || kind.len() > MAX_KIND_NAME || !self.knows(kind) {
            return Err(MaviError::validation("unknown_job_kind"));
        }
        Ok(())
    }
}

fn validate_payload(payload: &Value) -> Result<()> {
    if !payload.is_object() {
        return Err(MaviError::validation("job_payload_must_be_object"));
    }
    Ok(())
}

fn normalize_key(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_IDEMPOTENCY_KEY {
        return Err(MaviError::validation("job_idempotency_key_invalid"));
    }
    Ok(value.to_owned())
}

fn validate_worker(worker: &str) -> Result<()> {
    if worker.trim().is_empty() || worker.chars().count() > MAX_WORKER_NAME {
        return Err(MaviError::validation("job_worker_invalid"));
    }
    Ok(())
}

#[must_use]
pub fn retry_delay(attempts: i32) -> i64 {
    let exponent = u32::try_from(attempts.max(1)).unwrap_or(1).min(12);
    2_i64.saturating_pow(exponent).min(3_600)
}

fn interval_seconds(value: i64) -> f64 {
    f64::from(i32::try_from(value.clamp(1, 86_400)).unwrap_or(i32::MAX))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RecentCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes =
        serde_json::to_vec(&RecentCursor { created_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor(cursor: &Cursor) -> Result<RecentCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

fn row_to_job(row: &sqlx::postgres::PgRow) -> Result<Job> {
    Ok(Job {
        id: JobId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        kind: row.try_get("kind").map_err(|_| MaviError::Internal)?,
        payload: row.try_get("payload").map_err(|_| MaviError::Internal)?,
        state: JobState::parse(
            row.try_get::<String, _>("state")
                .map_err(|_| MaviError::Internal)?
                .as_str(),
        )?,
        run_at: row.try_get("run_at").map_err(|_| MaviError::Internal)?,
        claimed_until: row
            .try_get("claimed_until")
            .map_err(|_| MaviError::Internal)?,
        claimed_by: row.try_get("claimed_by").map_err(|_| MaviError::Internal)?,
        attempts: row.try_get("attempts").map_err(|_| MaviError::Internal)?,
        last_error: row.try_get("last_error").map_err(|_| MaviError::Internal)?,
        idempotency_key: row
            .try_get("idempotency_key")
            .map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        finished_at: row
            .try_get("finished_at")
            .map_err(|_| MaviError::Internal)?,
    })
}

async fn audit(
    tx: &mut SiteTx,
    context: &SiteContext,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
    payload: Value,
) -> Result<()> {
    AuditService
        .record(
            tx,
            context,
            &AuditEntry {
                action: action.to_owned(),
                resource_type: resource_type.to_owned(),
                resource_id: Some(resource_id),
                payload,
            },
        )
        .await
}

#[must_use]
pub fn api() -> mavi_contract::Api {
    let view = Permission {
        capability: Capability::Automation,
        action: Action::View,
    };
    let write = Permission {
        capability: Capability::Automation,
        action: Action::Write,
    };
    mavi_contract::Api::new(vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/jobs",
            "jobs.list",
            "List site jobs with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("JobListFilter")
        .returns(200, "JobPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/jobs/{id}",
            "jobs.read",
            "Read one site job",
        )
        .account_or_assistant()
        .requires(view)
        .returns(200, "Job")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/jobs/{id}/retry",
            "jobs.retry",
            "Retry a dead-letter job",
        )
        .account_or_assistant()
        .requires(write)
        .returns(200, "Job")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
    ])
    .with_shapes(shapes())
}

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "JobState",
            json!({"type":"string","enum":["ready","running","done","dead"]}),
        ),
        Shape::new(
            "JobListFilter",
            json!({
                "type":"object",
                "properties": {
                    "after":{"type":["string","null"],"maxLength":512},
                    "limit":{"type":"integer","minimum":1,"maximum":100},
                    "state":{"$ref":"#/components/schemas/JobState"},
                    "kind":{"type":["string","null"],"maxLength":120}
                }
            }),
        ),
        Shape::new(
            "Job",
            json!({
                "type":"object",
                "required":["id","kind","payload","state","run_at","claimed_until","claimed_by","attempts","last_error","idempotency_key","created_at","finished_at"],
                "properties": {
                    "id":{"type":"string","format":"uuid"},
                    "kind":{"type":"string"},
                    "payload":{"type":"object","additionalProperties":true},
                    "state":{"$ref":"#/components/schemas/JobState"},
                    "run_at":{"type":"string","format":"date-time"},
                    "claimed_until":{"type":["string","null"],"format":"date-time"},
                    "claimed_by":{"type":["string","null"]},
                    "attempts":{"type":"integer","minimum":0},
                    "last_error":{"type":["string","null"]},
                    "idempotency_key":{"type":["string","null"]},
                    "created_at":{"type":"string","format":"date-time"},
                    "finished_at":{"type":["string","null"],"format":"date-time"}
                }
            }),
        ),
        Shape::new(
            "JobPage",
            json!({"type":"object","required":["items","next_cursor"],"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/Job"}},"next_cursor":{"type":["string","null"]}}}),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEND: JobKind = JobKind::new("mail.send", 5);

    #[test]
    fn unknown_kinds_are_not_registered() {
        let jobs = JobsService::new([SEND]);
        assert!(jobs.knows("mail.send"));
        assert!(!jobs.knows("mail.renamed"));
        assert_eq!(jobs.max_attempts("mail.send"), Some(5));
    }

    #[test]
    fn retry_backoff_is_bounded() {
        assert_eq!(retry_delay(1), 2);
        assert_eq!(retry_delay(5), 32);
        assert_eq!(retry_delay(100), 3_600);
    }

    #[test]
    fn job_contract_uses_cursor_only_pagination() {
        let api = api();
        let filter = shapes()
            .into_iter()
            .find(|shape| shape.name == "JobListFilter")
            .expect("filter shape");
        let properties = filter.schema["properties"].as_object().expect("properties");
        assert!(properties.contains_key("after"));
        assert!(properties.contains_key("limit"));
        assert!(!properties.contains_key("offset"));
        assert!(!properties.contains_key("page"));
        api.validate().expect("valid API");
    }
}
