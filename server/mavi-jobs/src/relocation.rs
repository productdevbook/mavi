use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_core::{MaviError, Result, SiteContext, SiteId};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use super::{JobState, JobsService};

pub const JOBS_RELOCATION_FORMAT: &str = "mavi.jobs.relocation";
pub const JOBS_RELOCATION_VERSION: u16 = 1;
pub const MAX_JOBS_RELOCATION_RECORDS: usize = 100_000;
pub const MAX_JOBS_RELOCATION_BYTES: usize = 256 * 1024 * 1024;
const MAX_JOB_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobsRelocation {
    pub format: String,
    pub version: u16,
    pub source_site_id: SiteId,
    pub jobs: Vec<JobRelocation>,
}

/// A job lease is process-local state. Running jobs are exported as ready and
/// their claim owner/deadline are never copied to another runtime.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobRelocation {
    pub id: Uuid,
    pub kind: String,
    pub payload: Value,
    pub state: JobState,
    pub run_at: DateTime<Utc>,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub idempotency_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl JobsRelocation {
    #[must_use]
    pub fn empty(source_site_id: SiteId) -> Self {
        Self {
            format: JOBS_RELOCATION_FORMAT.to_owned(),
            version: JOBS_RELOCATION_VERSION,
            source_site_id,
            jobs: Vec::new(),
        }
    }

    pub fn validate_for_relocation(&self, target_site: SiteId) -> Result<()> {
        if self.format != JOBS_RELOCATION_FORMAT {
            return Err(MaviError::validation("jobs_relocation_format_invalid"));
        }
        if self.version != JOBS_RELOCATION_VERSION {
            return Err(MaviError::validation("jobs_relocation_version_unsupported"));
        }
        if self.source_site_id != target_site || self.source_site_id.into_uuid().is_nil() {
            return Err(MaviError::conflict("jobs_relocation_site_mismatch"));
        }
        if self.jobs.len() > MAX_JOBS_RELOCATION_RECORDS {
            return Err(MaviError::validation("jobs_relocation_counts_invalid"));
        }
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for job in &self.jobs {
            let valid_finished = match job.state {
                JobState::Ready | JobState::Running => job.finished_at.is_none(),
                JobState::Done | JobState::Dead => job.finished_at.is_some(),
            };
            if job.id.is_nil()
                || !ids.insert(job.id)
                || job.kind.trim().is_empty()
                || job.kind.chars().count() > 120
                || !job.payload.is_object()
                || serde_json::to_vec(&job.payload)
                    .map_err(|_| MaviError::Internal)?
                    .len()
                    > MAX_JOB_PAYLOAD_BYTES
                || job.attempts < 0
                || job
                    .last_error
                    .as_deref()
                    .is_some_and(|error| error.chars().count() > 4_000)
                || job
                    .idempotency_key
                    .as_deref()
                    .is_some_and(|key| key.is_empty() || key.chars().count() > 160)
                || !valid_finished
                || job
                    .idempotency_key
                    .as_ref()
                    .is_some_and(|key| !keys.insert((job.kind.clone(), key.clone())))
            {
                return Err(MaviError::validation("jobs_relocation_job_invalid"));
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| MaviError::Internal)?
            .len()
            > MAX_JOBS_RELOCATION_BYTES
        {
            return Err(MaviError::validation("jobs_relocation_too_large"));
        }
        Ok(())
    }

    pub fn record_count(&self) -> Result<i64> {
        i64::try_from(self.jobs.len())
            .map_err(|_| MaviError::validation("jobs_relocation_count_overflow"))
    }
}

impl JobsService {
    pub async fn export_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
    ) -> Result<JobsRelocation> {
        let rows = sqlx::query(
            "select id, kind, payload, state, run_at, attempts, last_error,
                    idempotency_key, created_at, finished_at
               from jobs where site_id = $1 order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let jobs = rows
            .iter()
            .map(|row| {
                let state = parse_state(
                    &row.try_get::<String, _>("state")
                        .map_err(|_| MaviError::Internal)?,
                )?;
                Ok(JobRelocation {
                    id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                    kind: row.try_get("kind").map_err(|_| MaviError::Internal)?,
                    payload: row.try_get("payload").map_err(|_| MaviError::Internal)?,
                    state: if state == JobState::Running {
                        JobState::Ready
                    } else {
                        state
                    },
                    run_at: row.try_get("run_at").map_err(|_| MaviError::Internal)?,
                    attempts: row.try_get("attempts").map_err(|_| MaviError::Internal)?,
                    last_error: row.try_get("last_error").map_err(|_| MaviError::Internal)?,
                    idempotency_key: row
                        .try_get("idempotency_key")
                        .map_err(|_| MaviError::Internal)?,
                    created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                    finished_at: if state == JobState::Running {
                        None
                    } else {
                        row.try_get("finished_at")
                            .map_err(|_| MaviError::Internal)?
                    },
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let relocation = JobsRelocation {
            format: JOBS_RELOCATION_FORMAT.to_owned(),
            version: JOBS_RELOCATION_VERSION,
            source_site_id: context.site_id,
            jobs,
        };
        relocation.validate_for_relocation(context.site_id)?;
        Ok(relocation)
    }

    pub async fn import_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        relocation: &JobsRelocation,
    ) -> Result<()> {
        relocation.validate_for_relocation(context.site_id)?;
        let site_id = context.site_id.into_uuid();
        sqlx::query("delete from jobs where site_id = $1")
            .bind(site_id)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        for job in &relocation.jobs {
            sqlx::query(
                "insert into jobs
                    (site_id, id, kind, payload, state, run_at, claimed_until,
                     claimed_by, attempts, last_error, idempotency_key, created_at, finished_at)
                 values ($1, $2, $3, $4, $5, $6, null, null, $7, $8, $9, $10, $11)",
            )
            .bind(site_id)
            .bind(job.id)
            .bind(&job.kind)
            .bind(&job.payload)
            .bind(job.state.as_str())
            .bind(job.run_at)
            .bind(job.attempts)
            .bind(&job.last_error)
            .bind(&job.idempotency_key)
            .bind(job.created_at)
            .bind(job.finished_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "portable.jobs.relocated".to_owned(),
                    resource_type: "JobsSnapshot".to_owned(),
                    resource_id: None,
                    payload: serde_json::json!({
                        "jobs": relocation.jobs.len(),
                        "leases": 0,
                        "running_normalized": true,
                    }),
                },
            )
            .await
    }
}

fn parse_state(value: &str) -> Result<JobState> {
    match value {
        "ready" => Ok(JobState::Ready),
        "running" => Ok(JobState::Running),
        "done" => Ok(JobState::Done),
        "dead" => Ok(JobState::Dead),
        _ => Err(MaviError::Internal),
    }
}
