//! Durable background execution for site-scoped Mavi jobs.
//!
//! Claiming happens in a short transaction, domain work happens while the
//! lease is held, and completion/failure is committed in the same transaction
//! as the domain result. Every worker mutation uses the explicit `system`
//! caller so background work never appears as anonymous public activity.

use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use chrono::{Duration as ChronoDuration, Utc};
use mavi_analytics::{ANALYTICS_RETENTION_JOB, AnalyticsRetentionJob, AnalyticsService};
use mavi_application::{
    DEFAULT_LEASE_SECONDS, HatchetBridgeClient, JobClaim, JobKind, JobState, LeaseOutcome,
    MAX_TRASH_RETENTION_BATCH, PluginService, TRASH_RETENTION_JOB, TrashRetentionJob, TrashService,
    WorkflowExecutor, WorkflowIntent, WorkflowScheduler, WorkflowService,
};
use mavi_audit::{AuditEntry, AuditService};
use mavi_content::{
    ContentService, SCHEDULED_PUBLISH_JOB, ScheduledPublishJob, ScheduledPublishOutcome,
};
use mavi_core::{
    DesignBuildId, JobId, MailListId, MailTemplateId, MaviError, PluginId, RequestId, Result,
    SiteContext, SiteId,
    ports::{FileStore, MailDeliveryPurpose, MailDeliveryRequest, Mailer, Seals},
};
use mavi_design::{
    BuildEngine, DESIGN_BUILD_FAILED, DESIGN_BUILD_IN_PROGRESS, DESIGN_BUILD_WORKFLOW,
    DesignService, StaticBuildEngine,
};
use mavi_flows::{
    FLOW_START_KIND, FLOW_STEP_KIND, FlowRun, FlowService, FlowStepInput, RecordStep, RunState,
    StartFlowJob, StepJob, StepKind, StepOutcome,
};
use mavi_forms::{FORM_RETENTION_JOB, FormRetentionJob, FormService};
use mavi_mail::{
    AddReader, ClaimedDelivery, EnqueueDelivery, MAX_DELIVERY_ATTEMPTS, MailDeliveryStatus,
    MailService,
};
use mavi_media::{
    MEDIA_CLEANUP_JOB, MEDIA_ORPHAN_CLEANUP_JOB, MEDIA_VARIANT_JOB, MediaCleanupJob,
    MediaOrphanCleanupJob, MediaService, MediaVariantJob, is_generated_media_storage_key,
    render_variant, variant_storage_key,
};
pub use mavi_observability::{WorkerMetrics, WorkerMetricsSnapshot};
use mavi_settings::SettingsService;
use mavi_storage::{Database, SiteTx};
use serde_json::{Map, Value, json};
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

/// Relays committed Rust workflow intents to the private Go Hatchet adapter.
/// Claiming is short and transactional; the network call happens after the
/// claim commit, so a process crash can safely redeliver the same idempotency
/// key.
#[derive(Clone, Debug)]
pub struct WorkflowRelay {
    database: Database,
    site_id: SiteId,
    worker_id: String,
    service: WorkflowService,
    plugins: PluginService,
    bridge: Option<HatchetBridgeClient>,
    poll_interval: Duration,
}

impl WorkflowRelay {
    #[must_use]
    pub fn new(
        database: Database,
        site_id: SiteId,
        worker_id: impl Into<String>,
        bridge: Option<HatchetBridgeClient>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            database,
            site_id,
            worker_id: worker_id.into(),
            service: WorkflowService,
            plugins: PluginService::default(),
            bridge,
            poll_interval,
        }
    }

    pub async fn run(&self) {
        loop {
            match self.run_once().await {
                Ok(true) => {}
                Ok(false) => tokio::time::sleep(self.poll_interval).await,
                Err(error) => {
                    tracing::error!(error = ?error, "workflow outbox relay failed");
                    tokio::time::sleep(self.poll_interval).await;
                }
            }
        }
    }

    pub async fn run_once(&self) -> Result<bool> {
        let Some(bridge) = self.bridge.as_ref() else {
            return Ok(false);
        };
        let context = SiteContext::system(self.site_id, self.worker_id.clone(), RequestId::new());
        let mut transaction = self.database.begin(&context).await?;
        self.plugins.invalidate();
        let active_plugins = self.plugins.enabled_set(&mut transaction).await?;
        let Some(intent) = self.service.claim(&mut transaction).await? else {
            transaction.commit().await?;
            return Ok(false);
        };
        if !active_plugins.contains(&intent.plugin) {
            self.service
                .cancel(&mut transaction, &intent.idempotency_key)
                .await?;
            transaction.commit().await?;
            tracing::info!(
                plugin = %intent.plugin,
                workflow = %intent.workflow,
                "cancelled workflow for disabled plugin"
            );
            return Ok(true);
        }
        transaction.commit().await?;

        match bridge.publish(&intent).await {
            Ok(run_id) => {
                let mut transaction = self.database.begin(&context).await?;
                self.service
                    .mark_published(&mut transaction, &intent, &run_id)
                    .await?;
                transaction.commit().await?;
            }
            Err(error) => {
                let mut transaction = self.database.begin(&context).await?;
                self.service
                    .mark_failed(&mut transaction, &intent, "hatchet_bridge_unavailable")
                    .await?;
                transaction.commit().await?;
                return Err(error);
            }
        }
        Ok(true)
    }
}

#[derive(Clone, Debug)]
pub struct WorkerSupervisor {
    database: Database,
    site_id: SiteId,
    jobs: WorkflowScheduler,
    plugins: PluginService,
    flows: FlowService,
    content: ContentService,
    analytics: AnalyticsService,
    forms: FormService,
    settings: SettingsService,
    media: MediaService,
    trash: TrashService,
    design: DesignService,
    file_store: Arc<dyn FileStore>,
    builder: Arc<dyn BuildEngine>,
    mail: MailService,
    mailer: Option<Arc<dyn Mailer>>,
    sealer: Option<Arc<dyn Seals>>,
    mail_first: Arc<AtomicBool>,
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
        Self::build(
            database,
            sites,
            config,
            file_store,
            Arc::new(StaticBuildEngine),
            None,
            None,
            metrics,
        )
    }

    /// Creates a supervisor that also drains the site-scoped mail outbox.
    ///
    /// The provider and keyring are injected by the composition root. The
    /// worker never discovers credentials, opens a provider transaction, or
    /// embeds a cloud SDK; it only claims a delivery, calls the [`Mailer`]
    /// port, and records the result.
    pub fn new_with_mailer(
        database: Database,
        sites: impl IntoIterator<Item = SiteId>,
        config: WorkerConfig,
        file_store: Arc<dyn FileStore>,
        mailer: Arc<dyn Mailer>,
        sealer: Arc<dyn Seals>,
    ) -> Self {
        Self::new_with_metrics_and_mailer(
            database,
            sites,
            config,
            file_store,
            mailer,
            sealer,
            WorkerMetrics::default(),
        )
    }

    /// Variant of [`Self::new_with_mailer`] that uses an existing metrics
    /// registry owned by the runtime composition root.
    pub fn new_with_metrics_and_mailer(
        database: Database,
        sites: impl IntoIterator<Item = SiteId>,
        config: WorkerConfig,
        file_store: Arc<dyn FileStore>,
        mailer: Arc<dyn Mailer>,
        sealer: Arc<dyn Seals>,
        metrics: WorkerMetrics,
    ) -> Self {
        Self::build(
            database,
            sites,
            config,
            file_store,
            Arc::new(StaticBuildEngine),
            Some(mailer),
            Some(sealer),
            metrics,
        )
    }

    /// Creates a supervisor with an injected design compiler. The compiler is
    /// invoked only after Hatchet has delivered the small design build intent;
    /// source files and artifacts never travel through Hatchet payloads.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_metrics_and_mailer_and_builder(
        database: Database,
        sites: impl IntoIterator<Item = SiteId>,
        config: WorkerConfig,
        file_store: Arc<dyn FileStore>,
        builder: Arc<dyn BuildEngine>,
        mailer: Arc<dyn Mailer>,
        sealer: Arc<dyn Seals>,
        metrics: WorkerMetrics,
    ) -> Self {
        Self::build(
            database,
            sites,
            config,
            file_store,
            builder,
            Some(mailer),
            Some(sealer),
            metrics,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        database: Database,
        sites: impl IntoIterator<Item = SiteId>,
        config: WorkerConfig,
        file_store: Arc<dyn FileStore>,
        builder: Arc<dyn BuildEngine>,
        mailer: Option<Arc<dyn Mailer>>,
        sealer: Option<Arc<dyn Seals>>,
        metrics: WorkerMetrics,
    ) -> Self {
        let mut sites = sites.into_iter();
        let site_id = sites
            .next()
            .expect("WorkerSupervisor requires exactly one site");
        assert!(
            sites.next().is_none(),
            "WorkerSupervisor cannot be constructed for multiple sites"
        );
        Self {
            database,
            site_id,
            jobs: WorkflowScheduler::new(mavi_flows::job_kinds().into_iter().chain([
                ANALYTICS_RETENTION_JOB,
                SCHEDULED_PUBLISH_JOB,
                MEDIA_CLEANUP_JOB,
                MEDIA_VARIANT_JOB,
                MEDIA_ORPHAN_CLEANUP_JOB,
                FORM_RETENTION_JOB,
                TRASH_RETENTION_JOB,
                JobKind::new(DESIGN_BUILD_WORKFLOW, 5),
            ])),
            plugins: PluginService::default(),
            flows: FlowService,
            content: ContentService,
            analytics: AnalyticsService,
            forms: FormService,
            settings: SettingsService,
            media: MediaService,
            trash: TrashService,
            design: DesignService,
            file_store,
            builder,
            mail: MailService,
            mailer,
            sealer,
            mail_first: Arc::new(AtomicBool::new(true)),
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

    /// Runs the polling compatibility loop for this fixed site forever.
    /// New production deployments use Hatchet delivery; this loop remains
    /// available to migration tooling and focused worker tests.
    pub async fn run(&self) {
        loop {
            match self.run_once(self.site_id).await {
                Ok(true) => {}
                Ok(false) => tokio::time::sleep(self.config.poll_interval).await,
                Err(error) => {
                    tracing::error!(site_id = %self.site_id, error = ?error, "background job poll failed");
                    tokio::time::sleep(self.config.poll_interval).await;
                }
            }
        }
    }

    /// Claims and executes at most one background item for a site.
    /// This method is intentionally public so self-host smoke tests and a
    /// future operator-managed supervisor can drive the exact same worker.
    pub async fn run_once(&self, site_id: SiteId) -> Result<bool> {
        if site_id != self.site_id {
            return Err(MaviError::Forbidden);
        }
        self.metrics.record_poll();
        let result = self.run_once_inner(site_id).await;
        if result.is_err() {
            self.metrics.record_error();
        }
        result
    }

    /// Executes the job referenced by a Hatchet intent. Hatchet owns delivery
    /// and retries; this method owns the site-scoped database/file mutation.
    /// A duplicate delivery is a no-op once the compatibility row is already
    /// done.
    pub async fn execute_intent(&self, intent: WorkflowIntent) -> Result<()> {
        if intent.site_id != self.site_id {
            return Err(MaviError::Forbidden);
        }
        let context = SiteContext::system(
            intent.site_id,
            self.config.worker_id.clone(),
            RequestId::from_uuid(intent.id),
        );
        let mut activation_transaction = self.database.begin(&context).await?;
        self.plugins.invalidate();
        let active_plugins = self
            .plugins
            .enabled_set(&mut activation_transaction)
            .await?;
        if !active_plugins.contains(&intent.plugin) {
            activation_transaction.commit().await?;
            // A plugin may be disabled after the relay has published an
            // intent. The site owner's decision turns this delivery into a
            // cancelled no-op; retained data remains intact and Hatchet does
            // not spend its retry budget on deliberately disabled work.
            let mut transaction = self.database.begin(&context).await?;
            match WorkflowService
                .get_run(&mut transaction, &intent.idempotency_key)
                .await
            {
                Ok(run) if !matches!(run.status.as_str(), "completed" | "cancelled") => {
                    WorkflowService
                        .cancel(&mut transaction, &intent.idempotency_key)
                        .await?;
                }
                Ok(_)
                | Err(MaviError::NotFound {
                    resource: "workflow_run",
                }) => {}
                Err(error) => return Err(error),
            }
            transaction.commit().await?;
            return Ok(());
        }
        if intent.workflow == "maintenance.tick" {
            self.enqueue_maintenance(&mut activation_transaction, &context, &active_plugins)
                .await?;
            activation_transaction.commit().await?;
            return Ok(());
        }
        if intent.workflow == "mail.delivery" {
            activation_transaction.commit().await?;
            return self.execute_mail_delivery_intent(&context, &intent).await;
        }
        let job_id = intent
            .payload
            .get("job_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .map(JobId::from_uuid)
            .ok_or_else(|| MaviError::validation("workflow_job_id_required"))?;
        activation_transaction.commit().await?;
        let mut transaction = self.database.begin(&context).await?;
        let claim = self
            .jobs
            .claim_by_id(
                &mut transaction,
                &self.config.worker_id,
                job_id,
                self.config.lease_seconds,
            )
            .await?;
        if let Some(claim) = claim {
            if claim.kind != intent.workflow {
                return Err(MaviError::validation("workflow_job_kind_mismatch"));
            }
            transaction.commit().await?;
            self.execute_claim(intent.site_id, claim).await?;
        } else {
            let run = WorkflowService
                .get_run(&mut transaction, &intent.idempotency_key)
                .await?;
            let job = self.jobs.get(&mut transaction, job_id).await?;
            transaction.commit().await?;
            if matches!(run.status.as_str(), "completed" | "cancelled" | "paused")
                || matches!(job.state, JobState::Done)
            {
                return Ok(());
            }
            if matches!(job.state, JobState::Running) {
                // Another Hatchet delivery currently owns the fenced DB
                // claim. Return a retryable conflict without reporting the
                // run as failed; the outer bridge will let Hatchet retry this
                // delivery after the active claim completes or expires.
                return Err(MaviError::conflict("workflow_execution_in_progress"));
            }
            return Err(MaviError::Internal);
        }

        let mut verification = self.database.begin(&context).await?;
        let job = self.jobs.get(&mut verification, job_id).await?;
        verification.commit().await?;
        if matches!(job.state, JobState::Done) {
            Ok(())
        } else {
            // `execute_claim` records a failed/dead compatibility row for the
            // legacy UI. A dead row still represents a failed delivery from
            // Hatchet's point of view, so return an error and let Hatchet
            // apply its retry/backoff policy. Only a completed mutation is a
            // successful task result.
            Err(MaviError::Internal)
        }
    }

    async fn run_once_inner(&self, site_id: SiteId) -> Result<bool> {
        let claim_context =
            SiteContext::system(site_id, self.config.worker_id.clone(), RequestId::new());
        let mut transaction = self.database.begin(&claim_context).await?;
        self.plugins.invalidate();
        let active_plugins = self.plugins.enabled_set(&mut transaction).await?;
        self.enqueue_maintenance(&mut transaction, &claim_context, &active_plugins)
            .await?;

        let mail_enabled = active_plugins.contains(&PluginId::Messaging);
        let (mail_claim, claim) =
            if mail_enabled && self.mail_first.fetch_xor(true, Ordering::Relaxed) {
                let mail_claim = self
                    .claim_mail_delivery(&mut transaction, &claim_context)
                    .await?;
                if mail_claim.is_some() {
                    (mail_claim, None)
                } else {
                    (
                        None,
                        self.claim_job(&mut transaction, &active_plugins).await?,
                    )
                }
            } else {
                let claim = self.claim_job(&mut transaction, &active_plugins).await?;
                if claim.is_some() {
                    (None, claim)
                } else if mail_enabled {
                    (
                        self.claim_mail_delivery(&mut transaction, &claim_context)
                            .await?,
                        None,
                    )
                } else {
                    (None, None)
                }
            };
        transaction.commit().await?;

        if let Some(claimed) = mail_claim {
            self.metrics.record_claim();
            // The mail row's idempotency key belongs to the provider request;
            // it is not necessarily the workflow key (manual requeues create
            // a fresh workflow key). The Hatchet path passes its exact intent
            // key below, while this legacy polling path intentionally leaves
            // workflow projection transitions to its caller.
            self.execute_mail_delivery(site_id, claimed, false, None)
                .await?;
            return Ok(true);
        }

        let Some(claim) = claim else {
            return Ok(false);
        };
        self.metrics.record_claim();
        self.execute_claim(site_id, claim).await?;
        Ok(true)
    }

    /// Enqueues periodic maintenance intents without claiming or executing a
    /// local job. Hatchet's `maintenance.tick` delivery uses this half only;
    /// the polling implementation below keeps the claim path for migration
    /// tooling and focused compatibility tests.
    async fn enqueue_maintenance(
        &self,
        transaction: &mut SiteTx,
        context: &SiteContext,
        active_plugins: &BTreeSet<PluginId>,
    ) -> Result<()> {
        if active_plugins.contains(&PluginId::Writing) {
            self.media
                .enqueue_next_cleanup(transaction, context, &self.jobs)
                .await?;
            self.media
                .enqueue_next_variant_job(transaction, context, &self.jobs)
                .await?;
            self.media
                .enqueue_orphan_cleanup_job(transaction, context, &self.jobs, Utc::now())
                .await?;
        }
        if active_plugins.contains(&PluginId::Forms) {
            self.forms
                .enqueue_retention_job(transaction, context, &self.jobs, Utc::now())
                .await?;
        }
        if active_plugins.contains(&PluginId::Analytics) {
            self.analytics
                .enqueue_retention_job(transaction, context, &self.jobs, Utc::now())
                .await?;
        }
        if active_plugins.contains(&PluginId::Governance) {
            self.trash
                .enqueue_retention_job(transaction, context, &self.jobs, Utc::now())
                .await?;
        }
        Ok(())
    }

    async fn claim_mail_delivery(
        &self,
        transaction: &mut SiteTx,
        context: &SiteContext,
    ) -> Result<Option<ClaimedDelivery>> {
        let Some(sealer) = self.sealer.as_deref() else {
            return Ok(None);
        };
        if self.mailer.is_none() {
            return Ok(None);
        }
        self.mail
            .claim_next_with_sealer(
                transaction,
                context,
                &self.config.worker_id,
                Utc::now() + ChronoDuration::seconds(self.config.lease_seconds),
                sealer,
            )
            .await
    }

    async fn claim_job(
        &self,
        transaction: &mut SiteTx,
        active_plugins: &BTreeSet<PluginId>,
    ) -> Result<Option<JobClaim>> {
        let mut claim = None;
        for kind in [
            FLOW_START_KIND.name,
            FLOW_STEP_KIND.name,
            SCHEDULED_PUBLISH_JOB.name,
            MEDIA_CLEANUP_JOB.name,
            MEDIA_VARIANT_JOB.name,
            MEDIA_ORPHAN_CLEANUP_JOB.name,
            DESIGN_BUILD_WORKFLOW,
            FORM_RETENTION_JOB.name,
            TRASH_RETENTION_JOB.name,
            // Retention is deliberately lowest priority: a newly discovered
            // analytics pass must not delay an already queued publish or
            // storage cleanup operation.
            ANALYTICS_RETENTION_JOB.name,
        ] {
            let Some(plugin) = workflow_plugin_for_kind(kind) else {
                continue;
            };
            if !active_plugins.contains(&plugin) {
                continue;
            }
            claim = self
                .jobs
                .claim(
                    transaction,
                    &self.config.worker_id,
                    &[kind],
                    self.config.lease_seconds,
                )
                .await?;
            if claim.is_some() {
                break;
            }
        }
        Ok(claim)
    }

    async fn execute_mail_delivery_intent(
        &self,
        context: &SiteContext,
        intent: &WorkflowIntent,
    ) -> Result<()> {
        let delivery_id = intent
            .payload
            .get("delivery_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .map(mavi_core::MailDeliveryId::from_uuid)
            .ok_or_else(|| MaviError::validation("workflow_delivery_id_required"))?;
        let sealer = self.sealer.as_deref().ok_or(MaviError::Internal)?;
        let mut transaction = self.database.begin(context).await?;
        let claimed = self
            .mail
            .claim_by_id_with_sealer(
                &mut transaction,
                context,
                &self.config.worker_id,
                delivery_id,
                Utc::now() + ChronoDuration::seconds(self.config.lease_seconds),
                sealer,
            )
            .await?;
        let Some(claimed) = claimed else {
            let delivery = self
                .mail
                .get_delivery(&mut transaction, context, delivery_id)
                .await?;
            let result = match delivery.status {
                MailDeliveryStatus::Sent => {
                    WorkflowService
                        .mark_completed(&mut transaction, &intent.idempotency_key)
                        .await?;
                    Ok(())
                }
                MailDeliveryStatus::Dead => {
                    WorkflowService
                        .mark_run_failed(&mut transaction, &intent.idempotency_key)
                        .await?;
                    Ok(())
                }
                MailDeliveryStatus::Cancelled => {
                    match WorkflowService
                        .get_run(&mut transaction, &intent.idempotency_key)
                        .await
                    {
                        Ok(run) if matches!(run.status.as_str(), "completed" | "cancelled") => {}
                        Ok(_) => {
                            WorkflowService
                                .cancel(&mut transaction, &intent.idempotency_key)
                                .await?;
                        }
                        Err(MaviError::NotFound {
                            resource: "workflow_run",
                        }) => {}
                        Err(error) => return Err(error),
                    }
                    Ok(())
                }
                MailDeliveryStatus::Queued
                | MailDeliveryStatus::Sending
                | MailDeliveryStatus::Retry => Err(MaviError::Internal),
            };
            transaction.commit().await?;
            return result;
        };
        transaction.commit().await?;
        self.execute_mail_delivery(
            context.site_id,
            claimed,
            true,
            Some(intent.idempotency_key.as_str()),
        )
        .await
    }

    async fn execute_mail_delivery(
        &self,
        site_id: SiteId,
        claimed: ClaimedDelivery,
        retry_with_hatchet: bool,
        workflow_key: Option<&str>,
    ) -> Result<()> {
        let context = SiteContext::system(
            site_id,
            self.config.worker_id.clone(),
            RequestId::from_uuid(claimed.delivery.id.into_uuid()),
        );
        let mailer = self.mailer.as_deref().ok_or(MaviError::Internal)?;
        let attempt_number =
            u16::try_from(claimed.attempt_number).map_err(|_| MaviError::Internal)?;
        let request = MailDeliveryRequest {
            delivery_id: claimed.delivery.id,
            attempt_number,
            idempotency_key: claimed.idempotency_key,
            purpose: match claimed.delivery.purpose {
                mavi_mail::MailPurpose::Transactional => MailDeliveryPurpose::Transactional,
                mavi_mail::MailPurpose::Campaign => MailDeliveryPurpose::Campaign,
            },
            sender: claimed.sender,
            message: claimed.message,
        };
        match mavi_mail::send_via(&context, mailer, request).await {
            Ok(receipt) => {
                let mut transaction = self.database.begin(&context).await?;
                self.mail
                    .mark_sent(
                        &mut transaction,
                        &context,
                        claimed.delivery.id,
                        &self.config.worker_id,
                        &receipt,
                    )
                    .await?;
                if let Some(workflow_key) = workflow_key {
                    WorkflowService
                        .mark_completed(&mut transaction, workflow_key)
                        .await?;
                }
                transaction.commit().await?;
                self.metrics.record_completed();
            }
            Err(error) => {
                let retry_at = mail_retry_at_for_error(&error, claimed.delivery.attempts);
                let error = format_mail_error(&error);
                let mut transaction = self.database.begin(&context).await?;
                let delivery = self
                    .mail
                    .mark_failed(
                        &mut transaction,
                        &context,
                        claimed.delivery.id,
                        &self.config.worker_id,
                        &error,
                        retry_at,
                    )
                    .await?;
                if let Some(workflow_key) = workflow_key
                    && delivery.status == MailDeliveryStatus::Dead
                {
                    WorkflowService
                        .mark_run_failed(&mut transaction, workflow_key)
                        .await?;
                }
                transaction.commit().await?;
                self.metrics.record_failed();
                if retry_with_hatchet && delivery.status == MailDeliveryStatus::Retry {
                    return Err(MaviError::Internal);
                }
            }
        }
        Ok(())
    }

    async fn execute_claim(&self, site_id: SiteId, claim: JobClaim) -> Result<()> {
        let context = SiteContext::system(
            site_id,
            self.config.worker_id.clone(),
            RequestId::from_uuid(claim.id.into_uuid()),
        );
        if claim.kind == FLOW_START_KIND.name {
            return self.execute_flow_start(&context, &claim).await;
        }
        if claim.kind == FLOW_STEP_KIND.name {
            return self.execute_flow_step(&context, &claim).await;
        }
        if claim.kind == MEDIA_CLEANUP_JOB.name {
            return self.execute_media_cleanup(&context, &claim).await;
        }
        if claim.kind == MEDIA_ORPHAN_CLEANUP_JOB.name {
            return self.execute_media_orphan_cleanup(&context, &claim).await;
        }
        if claim.kind == MEDIA_VARIANT_JOB.name {
            return self.execute_media_variant(&context, &claim).await;
        }
        if claim.kind == DESIGN_BUILD_WORKFLOW {
            return self.execute_design_build(&context, &claim).await;
        }
        if claim.kind == FORM_RETENTION_JOB.name {
            return self.execute_form_retention(&context, &claim).await;
        }
        if claim.kind == ANALYTICS_RETENTION_JOB.name {
            return self.execute_analytics_retention(&context, &claim).await;
        }
        if claim.kind == TRASH_RETENTION_JOB.name {
            return self.execute_trash_retention(&context, &claim).await;
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

    async fn execute_design_build(&self, context: &SiteContext, claim: &JobClaim) -> Result<()> {
        let build_id = claim
            .payload
            .get("build_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .map(DesignBuildId::from_uuid);
        let Some(build_id) = build_id else {
            return self
                .fail_claim(
                    context,
                    claim,
                    "invalid design build payload: build_id is required".to_owned(),
                )
                .await;
        };

        let mut transaction = self.database.begin(context).await?;
        let request = self
            .design
            .load_build_request(&mut transaction, context, build_id)
            .await?;
        let Some(request) = request else {
            // A duplicate Hatchet delivery may arrive after the build has
            // already reached a terminal state. Acknowledge it without
            // rebuilding or touching immutable artifacts.
            return self.complete_claim(transaction, context, claim).await;
        };
        transaction.commit().await?;

        let artifacts = match self.builder.build(context, build_id, &request.source).await {
            Ok(artifacts) => artifacts,
            Err(error) => {
                return self
                    .fail_design_build_attempt(context, claim, build_id, &error)
                    .await;
            }
        };
        let stored = match self
            .design
            .persist_artifacts(context, self.file_store.as_ref(), build_id, artifacts)
            .await
        {
            Ok(stored) => stored,
            Err(error) => {
                return self
                    .fail_design_build_attempt(context, claim, build_id, &error)
                    .await;
            }
        };

        let mut transaction = self.database.begin(context).await?;
        match self
            .design
            .finish_build_success(&mut transaction, context, build_id, &stored)
            .await
        {
            Ok(_) => self.complete_claim(transaction, context, claim).await,
            Err(MaviError::Conflict { code }) if code == DESIGN_BUILD_IN_PROGRESS => {
                // Another delivery won the build race after the source was
                // loaded. The deterministic artifact keys make the duplicate
                // write safe; only the winner changes the build row.
                self.complete_claim(transaction, context, claim).await
            }
            Err(error) => {
                drop(transaction);
                self.fail_design_build_attempt(context, claim, build_id, &error)
                    .await
            }
        }
    }

    async fn fail_design_build_attempt(
        &self,
        context: &SiteContext,
        claim: &JobClaim,
        build_id: DesignBuildId,
        error: &MaviError,
    ) -> Result<()> {
        let error_code = design_build_error_code(error);
        let max_attempts = i32::from(self.jobs.max_attempts(&claim.kind).unwrap_or(5));
        if claim.attempts >= max_attempts {
            let mut transaction = self.database.begin(context).await?;
            match self
                .design
                .finish_build_failed(&mut transaction, context, build_id, &error_code)
                .await
            {
                Ok(_) => return self.complete_claim(transaction, context, claim).await,
                Err(MaviError::Conflict { code }) if code == DESIGN_BUILD_IN_PROGRESS => {
                    return self.complete_claim(transaction, context, claim).await;
                }
                Err(error) => return Err(error),
            }
        }

        self.fail_claim(
            context,
            claim,
            format!("design build attempt failed: {error_code}"),
        )
        .await?;
        // The compatibility row is ready again and Hatchet owns the retry.
        Err(MaviError::Internal)
    }

    async fn execute_flow_start(&self, context: &SiteContext, claim: &JobClaim) -> Result<()> {
        let input = match serde_json::from_value::<StartFlowJob>(claim.payload.clone()) {
            Ok(mut input) => {
                if input.source_key.is_none() {
                    input.source_key = Some(format!("workflow:{}", claim.workflow_key));
                }
                input
            }
            Err(error) => {
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("invalid flow start payload: {error}"),
                    )
                    .await;
            }
        };
        let mut transaction = self.database.begin(context).await?;
        match self
            .flows
            .start(&mut transaction, context, &self.jobs, &input)
            .await
        {
            Ok(_) => self.complete_claim(transaction, context, claim).await,
            Err(error) => {
                drop(transaction);
                self.fail_claim(context, claim, format!("flow start failed: {error:?}"))
                    .await
            }
        }
    }

    async fn execute_flow_step(&self, context: &SiteContext, claim: &JobClaim) -> Result<()> {
        let input = match serde_json::from_value::<StepJob>(claim.payload.clone()) {
            Ok(input) => input,
            Err(error) => {
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("invalid flow step payload: {error}"),
                    )
                    .await;
            }
        };
        let mut transaction = self.database.begin(context).await?;
        let run = match self.flows.get_run(&mut transaction, input.run_id).await {
            Ok(run) => run,
            Err(error) => {
                drop(transaction);
                return self
                    .fail_claim(context, claim, format!("flow run load failed: {error:?}"))
                    .await;
            }
        };
        let Some(step) = run
            .definition
            .get(usize::try_from(input.position).unwrap_or(usize::MAX))
        else {
            drop(transaction);
            return self
                .fail_claim(context, claim, "flow step position invalid".to_owned())
                .await;
        };
        let step = step.clone();
        if matches!(run.state, RunState::Succeeded | RunState::Failed) {
            return self.complete_claim(transaction, context, claim).await;
        }
        transaction.commit().await?;

        match step.kind {
            StepKind::Wait => {
                let seconds = step
                    .config
                    .get("seconds")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| MaviError::validation("flow_wait_seconds_required"));
                let seconds = match seconds {
                    Ok(seconds) => seconds,
                    Err(error) => {
                        return self
                            .fail_claim(context, claim, format!("invalid flow wait: {error}"))
                            .await;
                    }
                };
                self.record_flow_step(
                    context,
                    claim,
                    RecordStep {
                        run_id: input.run_id,
                        position: input.position,
                        attempt: claim.attempts,
                        outcome: StepOutcome::Waiting,
                        detail: json!({"sleep_seconds": seconds}),
                        error: None,
                        next_at: Some(Utc::now() + ChronoDuration::seconds(seconds)),
                    },
                )
                .await
            }
            StepKind::SendMail => {
                self.execute_flow_send_mail(context, claim, &input, &run, &step)
                    .await
            }
            StepKind::AddToMailList => {
                self.execute_flow_add_to_mail_list(context, claim, &input, &run, &step)
                    .await
            }
            StepKind::Webhook => {
                if let Err(error) = self.execute_flow_webhook(&input, &run, &step).await {
                    return self
                        .fail_claim(context, claim, format!("flow webhook failed: {error:?}"))
                        .await;
                }
                self.record_flow_step(
                    context,
                    claim,
                    RecordStep {
                        run_id: input.run_id,
                        position: input.position,
                        attempt: claim.attempts,
                        outcome: StepOutcome::Succeeded,
                        detail: json!({"delivered": true}),
                        error: None,
                        next_at: None,
                    },
                )
                .await
            }
        }
    }

    async fn execute_flow_send_mail(
        &self,
        context: &SiteContext,
        claim: &JobClaim,
        input: &StepJob,
        run: &FlowRun,
        step: &FlowStepInput,
    ) -> Result<()> {
        let template_id =
            match flow_config_uuid(&step.config, "template_id", "flow_mail_template_required") {
                Ok(id) => MailTemplateId::from_uuid(id),
                Err(error) => {
                    return self
                        .fail_claim(context, claim, format!("invalid flow mail step: {error}"))
                        .await;
                }
            };
        let recipient = match flow_value_string(
            &step.config,
            "recipient",
            &run.event,
            &["recipient", "email"],
            "flow_mail_recipient_required",
        ) {
            Ok(recipient) => recipient,
            Err(error) => {
                return self
                    .fail_claim(context, claim, format!("invalid flow mail step: {error}"))
                    .await;
            }
        };
        let variables = flow_variables(&step.config, &run.event);
        let mut transaction = self.database.begin(context).await?;
        if let Err(error) = self
            .mail
            .enqueue_delivery(
                &mut transaction,
                context,
                &EnqueueDelivery {
                    recipient,
                    template_id,
                    variables,
                    idempotency_key: Some(format!("flow-mail:{}:{}", input.run_id, input.position)),
                },
            )
            .await
        {
            drop(transaction);
            return self
                .fail_claim(
                    context,
                    claim,
                    format!("flow mail enqueue failed: {error:?}"),
                )
                .await;
        }
        self.finish_flow_step(
            transaction,
            context,
            claim,
            RecordStep {
                run_id: input.run_id,
                position: input.position,
                attempt: claim.attempts,
                outcome: StepOutcome::Succeeded,
                detail: json!({"queued": true}),
                error: None,
                next_at: None,
            },
        )
        .await
    }

    async fn execute_flow_add_to_mail_list(
        &self,
        context: &SiteContext,
        claim: &JobClaim,
        input: &StepJob,
        run: &FlowRun,
        step: &FlowStepInput,
    ) -> Result<()> {
        let list_id = match flow_config_uuid(&step.config, "list_id", "flow_mail_list_required") {
            Ok(id) => MailListId::from_uuid(id),
            Err(error) => {
                return self
                    .fail_claim(context, claim, format!("invalid flow list step: {error}"))
                    .await;
            }
        };
        let email = match flow_value_string(
            &step.config,
            "email",
            &run.event,
            &["email", "recipient"],
            "flow_reader_email_required",
        ) {
            Ok(email) => email,
            Err(error) => {
                return self
                    .fail_claim(context, claim, format!("invalid flow list step: {error}"))
                    .await;
            }
        };
        let name = step
            .config
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| run.event.get("name").and_then(Value::as_str))
            .map(str::to_owned);
        let mut transaction = self.database.begin(context).await?;
        if let Err(error) = self
            .mail
            .add_reader(
                &mut transaction,
                context,
                list_id,
                &AddReader {
                    email,
                    name,
                    resubscribe: false,
                },
            )
            .await
        {
            drop(transaction);
            return self
                .fail_claim(
                    context,
                    claim,
                    format!("flow mail list enqueue failed: {error:?}"),
                )
                .await;
        }
        self.finish_flow_step(
            transaction,
            context,
            claim,
            RecordStep {
                run_id: input.run_id,
                position: input.position,
                attempt: claim.attempts,
                outcome: StepOutcome::Succeeded,
                detail: json!({"subscribed": true}),
                error: None,
                next_at: None,
            },
        )
        .await
    }

    async fn execute_flow_webhook(
        &self,
        input: &StepJob,
        run: &FlowRun,
        step: &FlowStepInput,
    ) -> Result<()> {
        let url = step
            .config
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| MaviError::validation("flow_webhook_url_required"))?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| MaviError::Internal)?;
        let response = client
            .post(url)
            .header("content-type", "application/json")
            .header(
                "idempotency-key",
                format!("flow-webhook:{}:{}", input.run_id, input.position),
            )
            .header("x-mavi-flow-run", input.run_id.to_string())
            .header("x-mavi-flow-step", input.position.to_string())
            .json(&run.event)
            .send()
            .await
            .map_err(|_| MaviError::Internal)?;
        if !response.status().is_success() {
            return Err(MaviError::Internal);
        }
        Ok(())
    }

    async fn record_flow_step(
        &self,
        context: &SiteContext,
        claim: &JobClaim,
        record: RecordStep,
    ) -> Result<()> {
        let transaction = self.database.begin(context).await?;
        self.finish_flow_step(transaction, context, claim, record)
            .await
    }

    async fn finish_flow_step(
        &self,
        mut transaction: SiteTx,
        context: &SiteContext,
        claim: &JobClaim,
        record: RecordStep,
    ) -> Result<()> {
        match self
            .flows
            .record_step(&mut transaction, context, &self.jobs, &record)
            .await
        {
            Ok(_) => self.complete_claim(transaction, context, claim).await,
            Err(error) => {
                drop(transaction);
                self.fail_claim(context, claim, format!("flow step failed: {error:?}"))
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

    async fn execute_form_retention(&self, context: &SiteContext, claim: &JobClaim) -> Result<()> {
        let payload = match serde_json::from_value::<FormRetentionJob>(claim.payload.clone()) {
            Ok(payload) if payload.bucket >= 0 => payload,
            Ok(_) => {
                return self
                    .fail_claim(context, claim, "invalid form retention bucket".to_owned())
                    .await;
            }
            Err(error) => {
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("invalid form retention payload: {error}"),
                    )
                    .await;
            }
        };

        let mut transaction = self.database.begin(context).await?;
        self.forms
            .prune_expired_submissions(&mut transaction, context, Utc::now(), payload.bucket)
            .await?;
        self.complete_claim(transaction, context, claim).await
    }

    async fn execute_analytics_retention(
        &self,
        context: &SiteContext,
        claim: &JobClaim,
    ) -> Result<()> {
        let payload = match serde_json::from_value::<AnalyticsRetentionJob>(claim.payload.clone()) {
            Ok(payload) if payload.bucket >= 0 => payload,
            Ok(_) => {
                return self
                    .fail_claim(
                        context,
                        claim,
                        "invalid analytics retention bucket".to_owned(),
                    )
                    .await;
            }
            Err(error) => {
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("invalid analytics retention payload: {error}"),
                    )
                    .await;
            }
        };

        let mut transaction = self.database.begin(context).await?;
        let policy = self
            .settings
            .analytics_retention(&mut transaction, context)
            .await?;
        self.analytics
            .prune_scheduled(&mut transaction, context, policy, payload.bucket)
            .await?;
        self.complete_claim(transaction, context, claim).await
    }

    async fn execute_trash_retention(&self, context: &SiteContext, claim: &JobClaim) -> Result<()> {
        let payload = match serde_json::from_value::<TrashRetentionJob>(claim.payload.clone()) {
            Ok(payload) if payload.bucket >= 0 => payload,
            Ok(_) => {
                return self
                    .fail_claim(context, claim, "invalid trash retention bucket".to_owned())
                    .await;
            }
            Err(error) => {
                return self
                    .fail_claim(
                        context,
                        claim,
                        format!("invalid trash retention payload: {error}"),
                    )
                    .await;
            }
        };

        let mut transaction = self.database.begin(context).await?;
        let policy = self
            .settings
            .trash_retention(&mut transaction, context)
            .await?;
        let cutoff = Utc::now() - ChronoDuration::days(i64::from(policy.days));
        let expired = self
            .trash
            .expired(&mut transaction, context, cutoff, MAX_TRASH_RETENTION_BATCH)
            .await?;

        let batch_size = expired.len();
        for item in expired {
            let deletion = match self
                .trash
                .permanently_delete(&mut transaction, context, item.kind, item.id)
                .await
            {
                Ok(deletion) => deletion,
                Err(MaviError::NotFound { .. }) => continue,
                Err(MaviError::Conflict { code }) => {
                    AuditService
                        .record(
                            &mut transaction,
                            context,
                            &AuditEntry {
                                action: "trash.retention.skipped".to_owned(),
                                resource_type: item.kind.resource_type().to_owned(),
                                resource_id: Some(item.id),
                                payload: serde_json::json!({
                                    "kind": item.kind,
                                    "reason": code,
                                }),
                            },
                        )
                        .await?;
                    continue;
                }
                Err(error) => return Err(error),
            };
            if let (Some(file_id), Some(storage_key)) =
                (deletion.file_id, deletion.file_storage_key)
            {
                self.media
                    .enqueue_cleanup_job(
                        &mut transaction,
                        context,
                        &self.jobs,
                        mavi_core::FileId::from_uuid(file_id),
                        &storage_key,
                    )
                    .await?;
            }
        }

        if batch_size == usize::try_from(MAX_TRASH_RETENTION_BATCH).unwrap_or_default() {
            self.trash
                .enqueue_retention_continuation(
                    &mut transaction,
                    context,
                    &self.jobs,
                    payload.bucket,
                    claim.id,
                )
                .await?;
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

impl WorkflowExecutor for WorkerSupervisor {
    fn execute(&self, intent: WorkflowIntent) -> mavi_core::ports::BoxFuture<'_, Result<()>> {
        Box::pin(async move { self.execute_intent(intent).await })
    }
}

fn workflow_plugin_for_kind(kind: &str) -> Option<PluginId> {
    if kind.starts_with("content.") || kind.starts_with("media.") || kind.starts_with("design.") {
        Some(PluginId::Writing)
    } else if kind.starts_with("forms.") {
        Some(PluginId::Forms)
    } else if kind.starts_with("analytics.") {
        Some(PluginId::Analytics)
    } else if kind.starts_with("trash.") {
        Some(PluginId::Governance)
    } else if kind.starts_with("automation.") {
        Some(PluginId::Automation)
    } else {
        None
    }
}

fn design_build_error_code(error: &MaviError) -> String {
    match error {
        MaviError::Validation { code, .. } | MaviError::Conflict { code } => code.clone(),
        _ => DESIGN_BUILD_FAILED.to_owned(),
    }
}

fn flow_config_uuid(config: &Value, key: &str, code: &str) -> Result<Uuid> {
    let value = config
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| MaviError::validation_field(code, key))?;
    Uuid::parse_str(value).map_err(|_| MaviError::validation_field(code, key))
}

fn flow_value_string(
    config: &Value,
    config_key: &str,
    event: &Value,
    event_keys: &[&str],
    code: &str,
) -> Result<String> {
    let value = config
        .get(config_key)
        .and_then(Value::as_str)
        .or_else(|| {
            event_keys
                .iter()
                .find_map(|key| event.get(*key).and_then(Value::as_str))
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| MaviError::validation_field(code, config_key))?;
    Ok(value.to_owned())
}

fn flow_variables(config: &Value, event: &Value) -> Map<String, Value> {
    let mut variables = event.as_object().cloned().unwrap_or_default();
    if let Some(configured) = config.get("variables").and_then(Value::as_object) {
        variables.extend(configured.clone());
    }
    variables
}

fn mail_retry_at_for_error(error: &MaviError, attempts: u16) -> Option<chrono::DateTime<Utc>> {
    mail_retry_at_for_error_at(error, attempts, Utc::now())
}

fn mail_retry_at_for_error_at(
    error: &MaviError,
    attempts: u16,
    now: chrono::DateTime<Utc>,
) -> Option<chrono::DateTime<Utc>> {
    if matches!(
        error,
        MaviError::Conflict { code }
            if code == "mail_sender_domain_not_allowed"
                || code == "mail_sender_not_configured"
    ) {
        return None;
    }
    let retry_at = mail_retry_at_from(now, attempts)?;
    let MaviError::ProviderRateLimited {
        retry_after_seconds,
    } = error
    else {
        return Some(retry_at);
    };
    let retry_after_seconds = (*retry_after_seconds).clamp(1, 86_400);
    Some(now + ChronoDuration::seconds(i64::try_from(retry_after_seconds).unwrap_or(86_400)))
}

fn mail_retry_at_from(now: chrono::DateTime<Utc>, attempts: u16) -> Option<chrono::DateTime<Utc>> {
    let max_attempts = u16::try_from(MAX_DELIVERY_ATTEMPTS).unwrap_or(u16::MAX);
    if attempts >= max_attempts {
        return None;
    }
    let exponent = u32::from(attempts.saturating_sub(1).min(6));
    let seconds = 2_i64.pow(exponent).min(3_600);
    Some(now + ChronoDuration::seconds(seconds))
}

fn format_mail_error(error: &MaviError) -> String {
    let message = format!("mail provider failed: {error:?}");
    let sanitized = message
        .chars()
        .filter(|character| !character.is_control())
        .take(1_900)
        .collect::<String>();
    if sanitized.is_empty() {
        "mail provider failed".to_owned()
    } else {
        sanitized
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

    #[test]
    fn worker_metrics_start_empty_and_are_copyable() {
        let metrics = WorkerMetrics::default();

        assert_eq!(metrics.snapshot(), WorkerMetricsSnapshot::default());
        assert_eq!(metrics.snapshot(), metrics.snapshot());
    }

    #[test]
    fn provider_rate_limit_uses_provider_delay_without_bypassing_attempt_limit() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let error = MaviError::ProviderRateLimited {
            retry_after_seconds: 120,
        };

        let retry_at = mail_retry_at_for_error_at(&error, 1, now).expect("retry");
        assert_eq!(retry_at, now + ChronoDuration::seconds(120));
        assert_eq!(
            mail_retry_at_from(now, 1),
            Some(now + ChronoDuration::seconds(1))
        );
        assert_eq!(mail_retry_at_from(now, MAX_DELIVERY_ATTEMPTS as u16), None);
    }

    #[test]
    fn ordinary_mail_errors_keep_exponential_retry() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);

        assert_eq!(
            mail_retry_at_from(now, 3),
            Some(now + ChronoDuration::seconds(4))
        );
    }

    #[test]
    fn sender_policy_failures_are_permanent_delivery_errors() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let error = MaviError::conflict("mail_sender_domain_not_allowed");

        assert_eq!(mail_retry_at_for_error_at(&error, 1, now), None);
    }
}
