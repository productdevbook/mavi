//! Compatibility-shaped workflow projection.
//!
//! Mavi no longer owns a second durable queue. Domain crates still receive a
//! small `WorkflowScheduler` port while they are migrated to application use-cases,
//! but every row written here is a `workflow_outbox` intent and every state
//! transition is reflected in `workflow_runs`. Hatchet owns delivery, retry,
//! timeout and concurrency; this crate only translates the old domain payload
//! shape into the workflow contract and exposes a read projection for older
//! callers. It is a compatibility module, not a second queue implementation.

use std::{collections::BTreeMap, sync::Arc};

use super::{WorkflowIntent, WorkflowService};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_core::{Cursor, JobId, MaviError, Page, PageRequest, PluginId, Result, SiteContext};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, QueryBuilder, Row};
use uuid::Uuid;

pub const DEFAULT_LEASE_SECONDS: i64 = 300;
pub const MAX_WORKER_NAME: usize = 160;
pub const MAX_KIND_NAME: usize = 120;
pub const MAX_IDEMPOTENCY_KEY: usize = 160;
pub const MAX_ERROR: usize = 4000;

/// A registered kind of durable work. Registration remains code-owned so an
/// intent can never name work no Rust executor understands.
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

/// The worker-facing shape is retained as a migration port. `claim_token` is
/// now an execution correlation value, not a database lease authority; the
/// actual delivery lease and fencing live in Hatchet.
#[derive(Clone, Debug)]
pub struct JobClaim {
    pub id: JobId,
    pub kind: String,
    pub payload: Value,
    pub attempts: i32,
    pub claimed_until: DateTime<Utc>,
    pub worker: String,
    pub claim_token: Uuid,
    pub workflow_key: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseOutcome {
    Completed,
    Lost,
}

#[derive(Clone, Debug)]
pub struct WorkflowScheduler {
    kinds: Arc<BTreeMap<String, u16>>,
}

impl WorkflowScheduler {
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

    /// Writes a workflow intent in the caller's transaction. The original
    /// payload is nested under `job_payload`; Hatchet receives only this small
    /// JSON input and Rust reloads any large domain records by ID.
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
        let job_id = idempotency_key
            .as_deref()
            .map_or_else(JobId::new, |key| deterministic_job_id(context, kind, key));
        let workflow_key = idempotency_key.as_deref().map_or_else(
            || format!("job:{kind}:{job_id}"),
            |key| format!("job:{kind}:{key}"),
        );
        let intent_payload = json!({
            "job_id": job_id,
            "job_payload": payload,
            "run_at": run_at.map(|value| value.to_rfc3339()),
            "job_idempotency_key": idempotency_key,
        });
        let intent = WorkflowIntent::new(
            context.site_id,
            workflow_plugin(kind),
            kind,
            workflow_key,
            intent_payload,
        )?;
        WorkflowService.enqueue(tx, &intent).await?;
        audit(
            tx,
            context,
            "jobs.enqueued",
            job_id,
            json!({"kind": kind, "idempotency_key": idempotency_key}),
        )
        .await?;
        Ok(job_id)
    }

    /// Compatibility polling for the maintenance executor. Normal delivery
    /// enters through `WorkflowRelay` and Hatchet; this method only advances a
    /// single already-persisted outbox intent when a maintenance tick asks
    /// Rust to process one due item.
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
        let row = sqlx::query(
            "select o.id, o.idempotency_key, o.workflow, o.payload,
                    r.attempts + 1 as execution_attempts, o.available_at
               from workflow_outbox o
               join workflow_runs r on r.site_id = o.site_id
                                    and r.idempotency_key = o.idempotency_key
              where o.site_id = $1
                and o.workflow = any($2)
                and o.status in ('pending', 'failed', 'publishing')
                and o.available_at <= now()
                and r.status not in ('completed', 'cancelled', 'paused')
                and (r.claim_until is null or r.claim_until <= now())
              order by o.available_at asc, o.created_at asc, o.id asc
              for update of o skip locked
              limit 1",
        )
        .bind(tx.site_id().into_uuid())
        .bind(names)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let workflow_key: String = row
            .try_get("idempotency_key")
            .map_err(|_| MaviError::Internal)?;
        let claim_token = Uuid::now_v7();
        let claimed_until = lease_deadline(lease_seconds);
        let claimed = sqlx::query(
            "update workflow_runs
                set status = 'running', attempts = attempts + 1,
                    claim_token = $3, claim_worker = $4, claim_until = $5,
                    updated_at = now()
              where site_id = $1 and idempotency_key = $2
                and status not in ('completed', 'cancelled', 'paused')
                and (claim_until is null or claim_until <= now())",
        )
        .bind(tx.site_id().into_uuid())
        .bind(&workflow_key)
        .bind(claim_token)
        .bind(worker)
        .bind(claimed_until)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if claimed.rows_affected() != 1 {
            return Ok(None);
        }
        sqlx::query(
            "update workflow_outbox
                set status = 'publishing', attempts = attempts + 1,
                    available_at = now() + make_interval(secs => $3)
              where site_id = $1 and idempotency_key = $2",
        )
        .bind(tx.site_id().into_uuid())
        .bind(&workflow_key)
        .bind(interval_seconds(lease_seconds))
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        claim_from_row(&row, worker, claim_token, claimed_until).map(Some)
    }

    /// Hatchet deliveries use the intent's job ID to load the same projection
    /// without creating a competing local lease. A completed/cancelled run is
    /// returned as `None`, making duplicate delivery a safe no-op.
    pub async fn claim_by_id(
        &self,
        tx: &mut SiteTx,
        worker: &str,
        id: JobId,
        lease_seconds: i64,
    ) -> Result<Option<JobClaim>> {
        validate_worker(worker)?;
        let row = sqlx::query(
            "select o.id, o.idempotency_key, o.workflow, o.payload,
                    r.attempts + 1 as execution_attempts, o.available_at
               from workflow_outbox o
               join workflow_runs r on r.site_id = o.site_id
                                    and r.idempotency_key = o.idempotency_key
              where o.site_id = $1
                and (o.id = $2 or o.payload->>'job_id' = $3)
                and r.status not in ('completed', 'cancelled', 'paused')
                and o.status not in ('cancelled', 'paused')
                and (r.claim_until is null or r.claim_until <= now())
              order by o.created_at asc
              for update of r skip locked
              limit 1",
        )
        .bind(tx.site_id().into_uuid())
        .bind(id.into_uuid())
        .bind(id.to_string())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let workflow_key: String = row
            .try_get("idempotency_key")
            .map_err(|_| MaviError::Internal)?;
        let claim_token = Uuid::now_v7();
        let claimed_until = lease_deadline(lease_seconds);
        let claimed = sqlx::query(
            "update workflow_runs
                set status = 'running', attempts = attempts + 1,
                    claim_token = $3, claim_worker = $4, claim_until = $5,
                    updated_at = now()
              where site_id = $1 and idempotency_key = $2
                and status not in ('completed', 'cancelled', 'paused')
                and (claim_until is null or claim_until <= now())",
        )
        .bind(tx.site_id().into_uuid())
        .bind(&workflow_key)
        .bind(claim_token)
        .bind(worker)
        .bind(claimed_until)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if claimed.rows_affected() != 1 {
            return Ok(None);
        }
        claim_from_row(&row, worker, claim_token, claimed_until).map(Some)
    }

    pub async fn heartbeat(
        &self,
        tx: &mut SiteTx,
        claim: &JobClaim,
        lease_seconds: i64,
    ) -> Result<LeaseOutcome> {
        let result = sqlx::query(
            "update workflow_runs
                set claim_until = $4, updated_at = now()
              where site_id = $1 and idempotency_key = $2
                and claim_token = $3 and claim_until > now()
                and status not in ('completed', 'cancelled', 'paused')",
        )
        .bind(tx.site_id().into_uuid())
        .bind(&claim.workflow_key)
        .bind(claim.claim_token)
        .bind(lease_deadline(lease_seconds))
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        Ok(if result.rows_affected() == 1 {
            LeaseOutcome::Completed
        } else {
            LeaseOutcome::Lost
        })
    }

    pub async fn complete(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        claim: &JobClaim,
    ) -> Result<LeaseOutcome> {
        let rows = sqlx::query(
            "update workflow_runs
                set status = 'completed', claim_token = null, claim_worker = null,
                    claim_until = null, updated_at = now()
              where site_id = $1 and idempotency_key = $2
                and claim_token = $3 and claim_until > now()
                and status not in ('completed', 'cancelled', 'paused')",
        )
        .bind(tx.site_id().into_uuid())
        .bind(&claim.workflow_key)
        .bind(claim.claim_token)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if rows.rows_affected() != 1 {
            return Ok(LeaseOutcome::Lost);
        }
        sqlx::query(
            "update workflow_outbox
                set status = 'published', published_at = coalesce(published_at, now()),
                    last_error = null
              where site_id = $1 and idempotency_key = $2
                and status not in ('cancelled', 'paused')",
        )
        .bind(tx.site_id().into_uuid())
        .bind(&claim.workflow_key)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        audit(tx, context, "jobs.completed", claim.id, json!({})).await?;
        Ok(LeaseOutcome::Completed)
    }

    pub async fn defer(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        claim: &JobClaim,
        run_at: DateTime<Utc>,
    ) -> Result<LeaseOutcome> {
        let rows = sqlx::query(
            "update workflow_runs
                set status = 'pending', claim_token = null, claim_worker = null,
                    claim_until = null, updated_at = now()
              where site_id = $1 and idempotency_key = $2
                and claim_token = $3 and claim_until > now()
                and status not in ('cancelled', 'paused')",
        )
        .bind(tx.site_id().into_uuid())
        .bind(&claim.workflow_key)
        .bind(claim.claim_token)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if rows.rows_affected() != 1 {
            return Ok(LeaseOutcome::Lost);
        }
        sqlx::query(
            "update workflow_outbox
                set status = 'pending', available_at = $3, last_error = null
              where site_id = $1 and idempotency_key = $2
                and status not in ('cancelled', 'paused')",
        )
        .bind(tx.site_id().into_uuid())
        .bind(&claim.workflow_key)
        .bind(run_at)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        audit(
            tx,
            context,
            "jobs.deferred",
            claim.id,
            json!({"run_at": run_at}),
        )
        .await?;
        Ok(LeaseOutcome::Completed)
    }

    pub async fn fail(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        claim: &JobClaim,
        error: &str,
    ) -> Result<LeaseOutcome> {
        let max_attempts = i32::from(
            self.max_attempts(&claim.kind)
                .ok_or_else(|| MaviError::validation("unknown_job_kind"))?,
        );
        let error = error.chars().take(MAX_ERROR).collect::<String>();
        let rows = sqlx::query(
            "update workflow_runs
                set status = 'failed', claim_token = null, claim_worker = null,
                    claim_until = null, updated_at = now()
              where site_id = $1 and idempotency_key = $2
                and claim_token = $3 and claim_until > now()
                and status not in ('cancelled', 'paused')",
        )
        .bind(tx.site_id().into_uuid())
        .bind(&claim.workflow_key)
        .bind(claim.claim_token)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if rows.rows_affected() != 1 {
            return Ok(LeaseOutcome::Lost);
        }
        sqlx::query(
            "update workflow_outbox
                set status = case when status in ('cancelled', 'paused')
                                  then status else 'published' end,
                    published_at = coalesce(published_at, now()),
                    last_error = $3
              where site_id = $1 and idempotency_key = $2
                and status <> 'cancelled'",
        )
        .bind(tx.site_id().into_uuid())
        .bind(&claim.workflow_key)
        .bind(&error)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        audit(
            tx,
            context,
            if claim.attempts >= max_attempts {
                "jobs.dead"
            } else {
                "jobs.failed"
            },
            claim.id,
            json!({"attempts": claim.attempts, "error": error}),
        )
        .await?;
        Ok(LeaseOutcome::Completed)
    }

    pub async fn retry(&self, tx: &mut SiteTx, context: &SiteContext, id: JobId) -> Result<Job> {
        self.retry_at(tx, context, id, Utc::now()).await
    }

    pub async fn retry_at(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: JobId,
        run_at: DateTime<Utc>,
    ) -> Result<Job> {
        let row = sqlx::query(
            "select o.id, o.idempotency_key, o.workflow, o.payload,
                    r.attempts as execution_attempts,
                    o.available_at, o.last_error, o.created_at,
                    r.status as run_status, r.updated_at, r.claim_until, r.claim_worker
               from workflow_outbox o
               join workflow_runs r on r.site_id = o.site_id
                                    and r.idempotency_key = o.idempotency_key
              where o.site_id = $1 and (o.id = $2 or o.payload->>'job_id' = $3)
                and r.status = 'failed'
              limit 1",
        )
        .bind(tx.site_id().into_uuid())
        .bind(id.into_uuid())
        .bind(id.to_string())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound { resource: "job" })?;
        let workflow_key: String = row
            .try_get("idempotency_key")
            .map_err(|_| MaviError::Internal)?;
        sqlx::query(
            "update workflow_outbox
                set status = 'pending', attempts = 0, available_at = $3,
                    last_error = null, published_at = null
              where site_id = $1 and idempotency_key = $2",
        )
        .bind(tx.site_id().into_uuid())
        .bind(&workflow_key)
        .bind(run_at)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        sqlx::query(
            "update workflow_runs set status = 'pending', attempts = 0,
                    claim_token = null, claim_worker = null, claim_until = null,
                    updated_at = now()
              where site_id = $1 and idempotency_key = $2",
        )
        .bind(tx.site_id().into_uuid())
        .bind(&workflow_key)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        // The row selected above is intentionally the failed projection used
        // to locate the retry target. Reload after both projection updates so
        // callers receive the new pending state and the requested cooldown,
        // rather than the stale failed snapshot.
        let job = self.get(tx, id).await?;
        audit(
            tx,
            context,
            "jobs.retried",
            job.id,
            json!({"run_at": run_at}),
        )
        .await?;
        Ok(job)
    }

    pub async fn get(&self, tx: &mut SiteTx, id: JobId) -> Result<Job> {
        let row = sqlx::query(
            "select o.id, o.idempotency_key, o.workflow, o.payload,
                    r.attempts as execution_attempts,
                    o.available_at, o.last_error, o.created_at,
                    r.status as run_status, r.updated_at, r.claim_until, r.claim_worker
               from workflow_outbox o
               join workflow_runs r on r.site_id = o.site_id
                                    and r.idempotency_key = o.idempotency_key
              where o.site_id = $1 and (o.id = $2 or o.payload->>'job_id' = $3)
              limit 1",
        )
        .bind(tx.site_id().into_uuid())
        .bind(id.into_uuid())
        .bind(id.to_string())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound { resource: "job" })?;
        job_from_row(&row)
    }

    pub async fn list(&self, tx: &mut SiteTx, filter: &JobListFilter) -> Result<Page<Job>> {
        if let Some(kind) = &filter.kind {
            self.validate_kind(kind)?;
        }
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = QueryBuilder::<Postgres>::new(
            "select o.id, o.idempotency_key, o.workflow, o.payload,
                    r.attempts as execution_attempts,
                    o.available_at, o.last_error, o.created_at,
                    r.status as run_status, r.updated_at, r.claim_until, r.claim_worker
               from workflow_outbox o
               join workflow_runs r on r.site_id = o.site_id
                                    and r.idempotency_key = o.idempotency_key
              where o.site_id = ",
        );
        query.push_bind(tx.site_id().into_uuid());
        if let Some(state) = filter.state {
            query.push(" and ");
            match state {
                JobState::Ready => query.push(
                    "(r.status = 'pending' or o.status in ('pending', 'failed', 'publishing'))",
                ),
                JobState::Running => query.push("r.status = 'running'"),
                JobState::Done => query.push("r.status = 'completed'"),
                JobState::Dead => query.push("r.status = 'failed'"),
            };
        }
        if let Some(kind) = &filter.kind {
            query.push(" and o.workflow = ").push_bind(kind);
        }
        if let Some(after) = after {
            query
                .push(" and (o.created_at, o.id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        query
            .push(" order by o.created_at desc, o.id desc limit ")
            .push_bind(limit + 1);
        let rows = query
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows.iter().map(job_from_row).collect::<Result<Vec<_>>>()?;
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

/// Source-compatible name for domain adapters that have not yet moved from
/// the old job vocabulary. It is an alias to the application workflow
/// projection, never a second queue or an alternate persistence model.
pub type JobsService = WorkflowScheduler;

fn workflow_plugin(kind: &str) -> PluginId {
    if kind.starts_with("media.") || kind.starts_with("content.") || kind.starts_with("design.") {
        PluginId::Writing
    } else if kind.starts_with("forms.") {
        PluginId::Forms
    } else if kind.starts_with("analytics.") {
        PluginId::Analytics
    } else if kind.starts_with("trash.") {
        PluginId::Governance
    } else if kind.starts_with("mail.") {
        PluginId::Messaging
    } else {
        PluginId::Automation
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

fn deterministic_job_id(context: &SiteContext, kind: &str, key: &str) -> JobId {
    let mut digest = Sha256::new();
    digest.update(context.site_id.into_uuid().as_bytes());
    digest.update(kind.as_bytes());
    digest.update([0]);
    digest.update(key.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.finalize()[..16]);
    // RFC 9562 variant/version bits make the projection a valid UUID while
    // retaining deterministic idempotency semantics.
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    JobId::from_uuid(Uuid::from_bytes(bytes))
}

fn claim_from_row(
    row: &sqlx::postgres::PgRow,
    worker: &str,
    claim_token: Uuid,
    claimed_until: DateTime<Utc>,
) -> Result<JobClaim> {
    let payload: Value = row.try_get("payload").map_err(|_| MaviError::Internal)?;
    let id = JobId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
    let job_id = payload
        .get("job_id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .map_or(id, JobId::from_uuid);
    Ok(JobClaim {
        id: job_id,
        kind: row.try_get("workflow").map_err(|_| MaviError::Internal)?,
        payload: payload.get("job_payload").cloned().unwrap_or(payload),
        attempts: row
            .try_get("execution_attempts")
            .map_err(|_| MaviError::Internal)?,
        claimed_until,
        worker: worker.to_owned(),
        claim_token,
        workflow_key: row
            .try_get("idempotency_key")
            .map_err(|_| MaviError::Internal)?,
    })
}

fn job_from_row(row: &sqlx::postgres::PgRow) -> Result<Job> {
    let payload: Value = row.try_get("payload").map_err(|_| MaviError::Internal)?;
    let id = JobId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
    let id = payload
        .get("job_id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .map_or(id, JobId::from_uuid);
    // `available_at` is the durable scheduling projection. The original
    // intent payload may contain a requested time, but workers can defer a
    // delivery (for example until a content publication time), so reading
    // the payload would expose a stale timestamp after that transition.
    let run_at: DateTime<Utc> = row
        .try_get("available_at")
        .map_err(|_| MaviError::Internal)?;
    let run_status: String = row.try_get("run_status").map_err(|_| MaviError::Internal)?;
    let state = match run_status.as_str() {
        "pending" | "cancelled" | "paused" => JobState::Ready,
        "running" => JobState::Running,
        "completed" => JobState::Done,
        "failed" => JobState::Dead,
        _ => return Err(MaviError::Internal),
    };
    Ok(Job {
        id,
        kind: row.try_get("workflow").map_err(|_| MaviError::Internal)?,
        payload: payload
            .get("job_payload")
            .cloned()
            .unwrap_or(payload.clone()),
        state,
        run_at,
        attempts: row
            .try_get("execution_attempts")
            .map_err(|_| MaviError::Internal)?,
        last_error: row.try_get("last_error").map_err(|_| MaviError::Internal)?,
        idempotency_key: payload
            .get("job_idempotency_key")
            .and_then(Value::as_str)
            .map(str::to_owned),
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        claimed_until: row.try_get("claim_until").ok(),
        claimed_by: row.try_get("claim_worker").ok(),
        finished_at: if matches!(state, JobState::Done | JobState::Dead) {
            row.try_get("updated_at").ok()
        } else {
            None
        },
    })
}

fn lease_deadline(lease_seconds: i64) -> DateTime<Utc> {
    Utc::now() + Duration::seconds(lease_seconds.clamp(1, 86_400))
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

async fn audit(
    tx: &mut SiteTx,
    context: &SiteContext,
    action: &str,
    id: JobId,
    payload: Value,
) -> Result<()> {
    AuditService
        .record(
            tx,
            context,
            &AuditEntry {
                action: action.to_owned(),
                resource_type: "WorkflowRun".to_owned(),
                resource_id: Some(id.into_uuid()),
                payload,
            },
        )
        .await
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
    fn idempotency_keys_have_stable_site_scoped_job_ids() {
        let site = SiteContext::public(mavi_core::SiteId::new());
        assert_eq!(
            deterministic_job_id(&site, "mail.send", "a"),
            deterministic_job_id(&site, "mail.send", "a")
        );
        assert_ne!(
            deterministic_job_id(&site, "mail.send", "a"),
            deterministic_job_id(&site, "mail.send", "b")
        );
    }
}
