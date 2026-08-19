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
use mavi_core::{MaviError, RequestId, Result, SiteContext, SiteId, ports::FileStore};
use mavi_jobs::{DEFAULT_LEASE_SECONDS, JobClaim, JobsService, LeaseOutcome};
use mavi_media::{
    MEDIA_CLEANUP_JOB, MEDIA_ORPHAN_CLEANUP_JOB, MEDIA_VARIANT_JOB, MediaCleanupJob,
    MediaOrphanCleanupJob, MediaService, MediaVariantJob, is_generated_media_storage_key,
    render_variant, variant_storage_key,
};
pub use mavi_observability::{WorkerMetrics, WorkerMetricsSnapshot};
use mavi_storage::{Database, SiteTx};
use serde_json::json;
use tokio::sync::RwLock;
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
    sites: Arc<RwLock<Arc<[SiteId]>>>,
    jobs: JobsService,
    content: ContentService,
    media: MediaService,
    file_store: Arc<dyn FileStore>,
    config: WorkerConfig,
    metrics: WorkerMetrics,
}

impl WorkerSupervisor {
    pub fn new(
        database: Database,
        sites: impl IntoIterator<Item = SiteId>,
        config: WorkerConfig,
        file_store: Arc<dyn FileStore>,
    ) -> Self {
        Self::new_with_metrics(
            database,
            sites,
            config,
            file_store,
            WorkerMetrics::default(),
        )
    }

    pub fn new_with_metrics(
        database: Database,
        sites: impl IntoIterator<Item = SiteId>,
        config: WorkerConfig,
        file_store: Arc<dyn FileStore>,
        metrics: WorkerMetrics,
    ) -> Self {
        Self {
            database,
            sites: Arc::new(RwLock::new(site_snapshot(sites))),
            jobs: JobsService::new([
                SCHEDULED_PUBLISH_JOB,
                MEDIA_CLEANUP_JOB,
                MEDIA_VARIANT_JOB,
                MEDIA_ORPHAN_CLEANUP_JOB,
            ]),
            content: ContentService,
            media: MediaService,
            file_store,
            config,
            metrics,
        }
    }

    #[must_use]
    pub fn config(&self) -> &WorkerConfig {
        &self.config
    }

    /// Returns the process-local counters shared by every site poll.
    #[must_use]
    pub fn metrics(&self) -> WorkerMetrics {
        self.metrics.clone()
    }

    /// Replaces the site directory as one snapshot.
    ///
    /// A cloud shard can therefore reconcile site lifecycle changes without
    /// rebuilding the router or starting one worker per site. A fixed-site
    /// runtime simply never needs to call this after construction.
    pub async fn replace_sites(&self, sites: impl IntoIterator<Item = SiteId>) {
        *self.sites.write().await = site_snapshot(sites);
    }

    /// Runs one polling loop over all configured sites forever. A transient
    /// site/database error is logged and isolated to that site; the next poll
    /// can recover without taking unrelated sites offline.
    pub async fn run(&self) {
        loop {
            let mut worked = false;
            let sites = self.sites.read().await.clone();
            for site_id in sites.iter().copied() {
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
        self.metrics.record_poll();
        let result = self.run_once_inner(site_id).await;
        if result.is_err() {
            self.metrics.record_error();
        }
        result
    }

    async fn run_once_inner(&self, site_id: SiteId) -> Result<bool> {
        let claim_context =
            SiteContext::system(site_id, self.config.worker_id.clone(), RequestId::new());
        let mut transaction = self.database.begin(&claim_context).await?;
        self.media
            .enqueue_next_cleanup(&mut transaction, &claim_context, &self.jobs)
            .await?;
        self.media
            .enqueue_next_variant_job(&mut transaction, &claim_context, &self.jobs)
            .await?;
        self.media
            .enqueue_orphan_cleanup_job(&mut transaction, &claim_context, &self.jobs, Utc::now())
            .await?;
        let mut claim = None;
        for kind in [
            SCHEDULED_PUBLISH_JOB.name,
            MEDIA_CLEANUP_JOB.name,
            MEDIA_VARIANT_JOB.name,
            MEDIA_ORPHAN_CLEANUP_JOB.name,
        ] {
            claim = self
                .jobs
                .claim(
                    &mut transaction,
                    &self.config.worker_id,
                    &[kind],
                    self.config.lease_seconds,
                )
                .await?;
            if claim.is_some() {
                break;
            }
        }
        transaction.commit().await?;

        let Some(claim) = claim else {
            return Ok(false);
        };
        self.metrics.record_claim();
        self.execute_claim(site_id, claim).await?;
        Ok(true)
    }

    async fn execute_claim(&self, site_id: SiteId, claim: JobClaim) -> Result<()> {
        let context = SiteContext::system(
            site_id,
            self.config.worker_id.clone(),
            RequestId::from_uuid(claim.id.into_uuid()),
        );
        if claim.kind == MEDIA_CLEANUP_JOB.name {
            return self.execute_media_cleanup(&context, &claim).await;
        }
        if claim.kind == MEDIA_ORPHAN_CLEANUP_JOB.name {
            return self.execute_media_orphan_cleanup(&context, &claim).await;
        }
        if claim.kind == MEDIA_VARIANT_JOB.name {
            return self.execute_media_variant(&context, &claim).await;
        }
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

    async fn execute_media_cleanup(&self, context: &SiteContext, claim: &JobClaim) -> Result<()> {
        let payload = match serde_json::from_value::<MediaCleanupJob>(claim.payload.clone()) {
            Ok(payload) => payload,
            Err(error) => {
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("invalid media cleanup payload: {error}"),
                    )
                    .await;
            }
        };

        for storage_key in
            std::iter::once(&payload.storage_key).chain(payload.additional_storage_keys.iter())
        {
            if let Err(error) = self.file_store.remove(context, storage_key).await {
                return self
                    .fail_claim(context, claim, format!("media cleanup failed: {error:?}"))
                    .await;
            }
        }

        let mut transaction = self.database.begin(context).await?;
        match self
            .media
            .complete_cleanup(
                &mut transaction,
                context,
                payload.file_id,
                &payload.storage_key,
            )
            .await
        {
            Ok(()) => self.complete_claim(transaction, context, claim).await,
            Err(error) => {
                drop(transaction);
                self.fail_claim(
                    context,
                    claim,
                    format!("media cleanup receipt failed: {error:?}"),
                )
                .await
            }
        }
    }

    async fn execute_media_orphan_cleanup(
        &self,
        context: &SiteContext,
        claim: &JobClaim,
    ) -> Result<()> {
        let bucket = match serde_json::from_value::<MediaOrphanCleanupJob>(claim.payload.clone()) {
            Ok(payload) if payload.bucket >= 0 => payload.bucket,
            Ok(_) => {
                return self
                    .fail_claim(context, claim, "invalid media orphan bucket".to_owned())
                    .await;
            }
            Err(error) => {
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("invalid media orphan payload: {error}"),
                    )
                    .await;
            }
        };

        let storage_keys = match self.file_store.list(context).await {
            Ok(storage_keys) => storage_keys,
            Err(error) => {
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("media storage list failed: {error:?}"),
                    )
                    .await;
            }
        };
        let mut transaction = self.database.begin(context).await?;
        let known = self
            .media
            .known_storage_keys(&mut transaction, context)
            .await?;
        transaction.commit().await?;

        let orphan_keys = storage_keys
            .into_iter()
            .filter(|key| is_generated_media_storage_key(key) && !known.contains(key))
            .collect::<Vec<_>>();

        for key in &orphan_keys {
            if let Err(error) = self.file_store.remove(context, key).await {
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("media orphan cleanup failed: {error:?}"),
                    )
                    .await;
            }
        }

        let mut transaction = self.database.begin(context).await?;
        if let Err(error) = self
            .media
            .record_orphan_cleanup(&mut transaction, context, orphan_keys.len(), bucket)
            .await
        {
            drop(transaction);
            return self
                .fail_claim(
                    context,
                    claim,
                    format!("media orphan cleanup receipt failed: {error:?}"),
                )
                .await;
        }

        self.complete_claim(transaction, context, claim).await
    }

    #[allow(clippy::too_many_lines)]
    async fn execute_media_variant(&self, context: &SiteContext, claim: &JobClaim) -> Result<()> {
        let payload = match serde_json::from_value::<MediaVariantJob>(claim.payload.clone()) {
            Ok(payload) => payload,
            Err(error) => {
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("invalid media variant payload: {error}"),
                    )
                    .await;
            }
        };

        let mut transaction = self.database.begin(context).await?;
        let source = self
            .media
            .variant_source(&mut transaction, context, payload.source_file_id)
            .await?;
        let Some(source) = source else {
            return self.complete_claim(transaction, context, claim).await;
        };
        transaction.commit().await?;

        let source_bytes = match self.file_store.get(context, &source.storage_key).await {
            Ok(source_bytes) => source_bytes,
            Err(error) => {
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("media variant source read failed: {error:?}"),
                    )
                    .await;
            }
        };
        let expected_bytes = match usize::try_from(source.bytes) {
            Ok(expected_bytes) => expected_bytes,
            Err(error) => {
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("media variant source size invalid: {error}"),
                    )
                    .await;
            }
        };
        if source_bytes.len() != expected_bytes
            || mavi_media::sha256_digest(&source_bytes) != source.sha256
        {
            return self
                .fail_claim(
                    context,
                    claim,
                    "media variant source integrity failed".to_owned(),
                )
                .await;
        }
        let preset = payload.preset;
        let rendered = match tokio::task::spawn_blocking(move || {
            render_variant(&source_bytes, preset)
        })
        .await
        {
            Ok(Ok(rendered)) => rendered,
            Ok(Err(error)) => {
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("media variant render failed: {error:?}"),
                    )
                    .await;
            }
            Err(error) => {
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("media variant worker panicked: {error}"),
                    )
                    .await;
            }
        };
        let candidate_key = variant_storage_key(payload.variant_id);
        if let Err(error) = self
            .file_store
            .put(context, &candidate_key, rendered.content.clone())
            .await
        {
            return self
                .fail_claim(
                    context,
                    claim,
                    format!("media variant write failed: {error:?}"),
                )
                .await;
        }

        let mut transaction = self.database.begin(context).await?;
        let owned_key = match self
            .media
            .finalize_variant(
                &mut transaction,
                context,
                &payload,
                &candidate_key,
                &rendered,
            )
            .await
        {
            Ok(owned_key) => owned_key,
            Err(error) => {
                drop(transaction);
                let _ = self.file_store.remove(context, &candidate_key).await;
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("media variant metadata failed: {error:?}"),
                    )
                    .await;
            }
        };
        if owned_key.as_deref() != Some(candidate_key.as_str())
            && let Err(error) = self.file_store.remove(context, &candidate_key).await
        {
            drop(transaction);
            return self
                .fail_claim(
                    context,
                    claim,
                    format!("media variant candidate cleanup failed: {error:?}"),
                )
                .await;
        }
        self.complete_claim(transaction, context, claim).await
    }

    async fn complete_claim(
        &self,
        mut transaction: SiteTx,
        context: &SiteContext,
        claim: &JobClaim,
    ) -> Result<()> {
        match self.jobs.complete(&mut transaction, context, claim).await? {
            LeaseOutcome::Completed => {
                self.metrics.record_completed();
                transaction.commit().await?;
            }
            LeaseOutcome::Lost => {
                self.metrics.record_lost_lease();
            }
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
        match self
            .jobs
            .defer(&mut transaction, context, claim, run_at)
            .await?
        {
            LeaseOutcome::Completed => {
                self.metrics.record_deferred();
                transaction.commit().await?;
            }
            LeaseOutcome::Lost => {
                self.metrics.record_lost_lease();
            }
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
        match self
            .jobs
            .fail(&mut transaction, context, claim, &error)
            .await?
        {
            LeaseOutcome::Completed => {
                self.metrics.record_failed();
                transaction.commit().await?;
            }
            LeaseOutcome::Lost => {
                self.metrics.record_lost_lease();
            }
        }
        Ok(())
    }
}

fn site_snapshot(sites: impl IntoIterator<Item = SiteId>) -> Arc<[SiteId]> {
    Arc::from(sites.into_iter().collect::<Vec<_>>())
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

    #[test]
    fn worker_metrics_start_empty_and_are_copyable() {
        let metrics = WorkerMetrics::default();

        assert_eq!(metrics.snapshot(), WorkerMetricsSnapshot::default());
        assert_eq!(metrics.snapshot(), metrics.snapshot());
    }

    #[test]
    fn a_site_directory_is_kept_as_one_snapshot() {
        let first = SiteId::new();
        let second = SiteId::new();
        let snapshot = site_snapshot([first, second]);

        assert_eq!(snapshot.as_ref(), &[first, second]);
    }
}
