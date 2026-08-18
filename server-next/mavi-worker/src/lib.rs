//! Durable background execution for site-scoped Mavi jobs.
//!
//! Claiming happens in a short transaction, domain work happens while the
//! lease is held, and completion/failure is committed in the same transaction
//! as the domain result. Every worker mutation uses the explicit `system`
//! caller so background work never appears as anonymous public activity.

use std::{sync::Arc, time::Duration};

use chrono::Utc;
use mavi_audit::{AuditEntry, AuditService};
use mavi_content::{
    ContentService, SCHEDULED_PUBLISH_JOB, ScheduledPublishJob, ScheduledPublishOutcome,
};
use mavi_core::{MaviError, RequestId, Result, SiteContext, SiteId};
use mavi_jobs::{DEFAULT_LEASE_SECONDS, JobClaim, JobsService, LeaseOutcome};
use mavi_storage::{Database, SiteTx};
use serde_json::json;
use uuid::Uuid;

pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub struct WorkerConfig {
    pub worker_id: String,
    pub lease_seconds: i64,
    pub poll_interval: Duration,
}

impl WorkerConfig {
    pub fn new(
        worker_id: impl Into<String>,
        lease_seconds: i64,
        poll_interval: Duration,
    ) -> Result<Self> {
        let worker_id = worker_id.into();
        if worker_id.trim().is_empty() || worker_id.chars().count() > 160 {
            return Err(MaviError::validation("worker_id_invalid"));
        }
        if lease_seconds < 1 {
            return Err(MaviError::validation("worker_lease_invalid"));
        }
        if poll_interval.is_zero() {
            return Err(MaviError::validation("worker_poll_interval_invalid"));
        }
        Ok(Self {
            worker_id,
            lease_seconds,
            poll_interval,
        })
    }
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            worker_id: format!("mavi-content-worker-{}", Uuid::now_v7()),
            lease_seconds: DEFAULT_LEASE_SECONDS,
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }
}

#[derive(Clone, Debug)]
pub struct WorkerSupervisor {
    database: Database,
    sites: Arc<[SiteId]>,
    jobs: JobsService,
    content: ContentService,
    config: WorkerConfig,
}

impl WorkerSupervisor {
    pub fn new(
        database: Database,
        sites: impl IntoIterator<Item = SiteId>,
        config: WorkerConfig,
    ) -> Self {
        Self {
            database,
            sites: Arc::from(sites.into_iter().collect::<Vec<_>>()),
            jobs: JobsService::new([SCHEDULED_PUBLISH_JOB]),
            content: ContentService,
            config,
        }
    }

    #[must_use]
    pub fn config(&self) -> &WorkerConfig {
        &self.config
    }

    /// Runs one polling loop over all configured sites forever. A transient
    /// site/database error is logged and isolated to that site; the next poll
    /// can recover without taking unrelated sites offline.
    pub async fn run(&self) {
        loop {
            let mut worked = false;
            for site_id in self.sites.iter().copied() {
                match self.run_once(site_id).await {
                    Ok(processed) => worked |= processed,
                    Err(error) => {
                        tracing::error!(%site_id, error = ?error, "background job poll failed");
                    }
                }
            }
            if !worked {
                tokio::time::sleep(self.config.poll_interval).await;
            }
        }
    }

    /// Claims and executes at most one scheduled publication for a site.
    /// This method is intentionally public so self-host smoke tests and a
    /// future operator-managed supervisor can drive the exact same worker.
    pub async fn run_once(&self, site_id: SiteId) -> Result<bool> {
        let claim_context =
            SiteContext::system(site_id, self.config.worker_id.clone(), RequestId::new());
        let mut transaction = self.database.begin(&claim_context).await?;
        let claim = self
            .jobs
            .claim(
                &mut transaction,
                &self.config.worker_id,
                &[SCHEDULED_PUBLISH_JOB.name],
                self.config.lease_seconds,
            )
            .await?;
        transaction.commit().await?;

        let Some(claim) = claim else {
            return Ok(false);
        };
        self.execute_claim(site_id, claim).await?;
        Ok(true)
    }

    async fn execute_claim(&self, site_id: SiteId, claim: JobClaim) -> Result<()> {
        let context = SiteContext::system(
            site_id,
            self.config.worker_id.clone(),
            RequestId::from_uuid(claim.id.into_uuid()),
        );
        let payload = match serde_json::from_value::<ScheduledPublishJob>(claim.payload.clone()) {
            Ok(payload) => payload,
            Err(error) => {
                return self
                    .fail_claim(
                        &context,
                        &claim,
                        format!("invalid scheduled payload: {error}"),
                    )
                    .await;
            }
        };

        if payload.scheduled_at > Utc::now() {
            return self
                .defer_claim(&context, &claim, payload.scheduled_at)
                .await;
        }

        let mut transaction = self.database.begin(&context).await?;
        match self
            .content
            .publish_scheduled(
                &mut transaction,
                &context,
                payload.content_id,
                payload.scheduled_at,
                Utc::now(),
            )
            .await
        {
            Ok(ScheduledPublishOutcome::Published(_)) => {
                self.complete_claim(transaction, &context, &claim).await
            }
            Ok(ScheduledPublishOutcome::Skipped(reason)) => {
                AuditService
                    .record(
                        &mut transaction,
                        &context,
                        &AuditEntry {
                            action: "content.publish_scheduled_skipped".to_owned(),
                            resource_type: "Content".to_owned(),
                            resource_id: Some(payload.content_id.into_uuid()),
                            payload: json!({
                                "scheduled_at": payload.scheduled_at,
                                "reason": reason.as_str(),
                            }),
                        },
                    )
                    .await?;
                self.complete_claim(transaction, &context, &claim).await
            }
            Err(error) => {
                drop(transaction);
                self.fail_claim(
                    &context,
                    &claim,
                    format!("content publish failed: {error:?}"),
                )
                .await
            }
        }
    }

    async fn complete_claim(
        &self,
        mut transaction: SiteTx,
        context: &SiteContext,
        claim: &JobClaim,
    ) -> Result<()> {
        if self
            .jobs
            .complete(&mut transaction, context, claim.id, &self.config.worker_id)
            .await?
            == LeaseOutcome::Completed
        {
            transaction.commit().await?;
        }
        Ok(())
    }

    async fn defer_claim(
        &self,
        context: &SiteContext,
        claim: &JobClaim,
        run_at: chrono::DateTime<Utc>,
    ) -> Result<()> {
        let mut transaction = self.database.begin(context).await?;
        if self
            .jobs
            .defer(
                &mut transaction,
                context,
                claim.id,
                &self.config.worker_id,
                run_at,
            )
            .await?
            == LeaseOutcome::Completed
        {
            transaction.commit().await?;
        }
        Ok(())
    }

    async fn fail_claim(
        &self,
        context: &SiteContext,
        claim: &JobClaim,
        error: String,
    ) -> Result<()> {
        let mut transaction = self.database.begin(context).await?;
        if self
            .jobs
            .fail(
                &mut transaction,
                context,
                claim,
                &self.config.worker_id,
                &error,
            )
            .await?
            == LeaseOutcome::Completed
        {
            transaction.commit().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_config_requires_a_unique_non_empty_identity() {
        assert!(WorkerConfig::new("", 30, Duration::from_secs(1)).is_err());
        assert!(WorkerConfig::new("worker", 0, Duration::from_secs(1)).is_err());
        assert!(WorkerConfig::new("worker", 30, Duration::ZERO).is_err());
        let config = WorkerConfig::new("worker-a", 30, Duration::from_secs(1)).expect("config");
        assert_eq!(config.worker_id, "worker-a");
    }

    #[test]
    fn default_worker_identity_is_not_shared_between_instances() {
        let first = WorkerConfig::default();
        let second = WorkerConfig::default();
        assert_ne!(first.worker_id, second.worker_id);
    }
}
