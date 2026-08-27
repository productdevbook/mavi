//! Application-layer orchestration for the single-site Mavi runtime.
//!
//! Domain crates own their data and local invariants. This crate owns
//! cross-domain decisions: plugin lifecycle, authorization entry points and
//! durable workflow intent. HTTP and workers depend on these ports instead of
//! reaching into domain internals.

use std::{
    collections::BTreeSet,
    sync::{Arc, RwLock},
};

use base64::Engine as _;
use chrono::{DateTime, Utc};
use mavi_authz::CedarAuthorizer;
use mavi_core::{
    Cursor, Grant, Grants, MaviError, Page, PageRequest, Permission, PluginId, Result, SiteContext,
    SiteId,
    ports::{AuthorizationObserver, AuthorizationOutcome, BoxFuture},
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, QueryBuilder, Row};
use uuid::Uuid;

pub mod jobs;
pub mod portable;
pub mod trash;

pub use portable::{ImportReceipt, PortableBundle, PortableImportRequest, PortableService};

pub use trash::{
    ExpiredTrashItem, MAX_TRASH_RETENTION_BATCH, PermanentDeletion, TRASH_FLOW_ACTIVE_WORK,
    TRASH_ITEM_NOT_FOUND, TRASH_KIND_INVALID, TRASH_RESTORE_CONFLICT,
    TRASH_RETENTION_BUCKET_SECONDS, TRASH_RETENTION_JOB, TRASH_SHOP_PRODUCT_ACTIVE_HOLD, TrashItem,
    TrashKind, TrashListFilter, TrashRetentionJob, TrashService,
};

pub mod plugins {
    use super::{
        Arc, BTreeSet, DateTime, MaviError, Permission, PluginId, Result, Row, RwLock, Serialize,
        SiteTx, Utc, Value,
    };

    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct NavigationItem {
        pub label: String,
        pub href: String,
    }

    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct PluginDescriptor {
        pub id: PluginId,
        pub version: String,
        pub dependencies: Vec<PluginId>,
        pub default_enabled: bool,
        pub routes: Vec<String>,
        pub permissions: Vec<Permission>,
        pub navigation: Vec<NavigationItem>,
        pub workflows: Vec<String>,
        /// The policy fragment is exposed for startup/CI validation. Cedar
        /// remains embedded in `mavi-authz`; this is metadata owned by the
        /// compiled plugin, not executable input from the database.
        pub cedar_policy: String,
    }

    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct PluginRecord {
        pub id: PluginId,
        pub version: String,
        pub dependencies: Vec<PluginId>,
        pub default_enabled: bool,
        pub enabled: bool,
        pub config: Value,
        pub updated_at: DateTime<Utc>,
    }

    #[derive(Clone, Debug)]
    pub struct PluginRegistry {
        descriptors: Vec<PluginDescriptor>,
    }

    impl Default for PluginRegistry {
        fn default() -> Self {
            Self::built_in()
        }
    }

    impl PluginRegistry {
        #[must_use]
        #[allow(clippy::too_many_lines)]
        pub fn built_in() -> Self {
            Self {
                descriptors: vec![
                    descriptor(
                        PluginId::Core,
                        true,
                        &[],
                        &["/api/v1/plugins", "/api/v1/auth", "/api/v1/people"],
                        PluginId::Core.business_actions(),
                        &["/dashboard", "/dashboard/plugins"],
                        &["maintenance.tick"],
                    ),
                    descriptor(
                        PluginId::Writing,
                        true,
                        &[PluginId::Core],
                        &[
                            "/api/v1/content",
                            "/api/v1/content-types",
                            "/api/v1/terms",
                            "/api/v1/files",
                            "/api/v1/design",
                            "/preview/v1/design",
                            "/public/v1/content",
                            "/public/v1/terms",
                            "/public/v1/files",
                            "/public/v1/site",
                        ],
                        PluginId::Writing.business_actions(),
                        &[
                            "/dashboard/content",
                            "/dashboard/content-types",
                            "/dashboard/media",
                            "/dashboard/design",
                        ],
                        &[
                            "content.publish_scheduled",
                            "media.cleanup",
                            "media.variant_generate",
                            "media.orphan_cleanup",
                            "design.build",
                        ],
                    ),
                    descriptor(
                        PluginId::Commerce,
                        false,
                        &[PluginId::Core],
                        &["/api/v1/shop", "/public/v1/shop"],
                        PluginId::Commerce.business_actions(),
                        &["/dashboard/shop"],
                        &["shop.order.fulfillment"],
                    ),
                    descriptor(
                        PluginId::Learning,
                        false,
                        &[PluginId::Core],
                        &[
                            "/api/v1/courses",
                            "/public/v1/courses",
                            "/student/v1/learning",
                            "/student/v1/auth",
                        ],
                        PluginId::Learning.business_actions(),
                        &["/dashboard/courses"],
                        &["courses.lesson.publish"],
                    ),
                    descriptor(
                        PluginId::Forms,
                        false,
                        &[PluginId::Core],
                        &[
                            "/api/v1/forms",
                            "/api/v1/form-submissions",
                            "/public/v1/forms",
                        ],
                        PluginId::Forms.business_actions(),
                        &["/dashboard/forms"],
                        &["forms.retention"],
                    ),
                    descriptor(
                        PluginId::Messaging,
                        false,
                        &[PluginId::Core],
                        &["/api/v1/mail", "/public/v1/mail", "/internal/v1/mail"],
                        PluginId::Messaging.business_actions(),
                        &["/dashboard/mail"],
                        &["mail.delivery"],
                    ),
                    descriptor(
                        PluginId::Automation,
                        false,
                        &[PluginId::Core],
                        &["/api/v1/automation", "/api/v1/workflows"],
                        PluginId::Automation.business_actions(),
                        &["/dashboard/automation"],
                        &["automation.flow.start", "automation.flow.step"],
                    ),
                    descriptor(
                        PluginId::Boards,
                        false,
                        &[PluginId::Core],
                        &["/api/v1/boards"],
                        PluginId::Boards.business_actions(),
                        &["/dashboard/boards"],
                        &[],
                    ),
                    descriptor(
                        PluginId::Analytics,
                        false,
                        &[PluginId::Core],
                        &["/api/v1/analytics", "/public/v1/analytics"],
                        PluginId::Analytics.business_actions(),
                        &["/dashboard/analytics"],
                        &["analytics.retention"],
                    ),
                    descriptor(
                        PluginId::Governance,
                        false,
                        &[PluginId::Core],
                        &["/api/v1/audit", "/api/v1/trash", "/api/v1/portable"],
                        PluginId::Governance.business_actions(),
                        &["/dashboard/audit", "/dashboard/trash"],
                        &["trash.retention"],
                    ),
                ],
            }
        }

        #[must_use]
        pub fn descriptors(&self) -> &[PluginDescriptor] {
            &self.descriptors
        }

        #[must_use]
        pub fn descriptor(&self, id: PluginId) -> Option<&PluginDescriptor> {
            self.descriptors
                .iter()
                .find(|descriptor| descriptor.id == id)
        }

        #[must_use]
        pub fn default_enabled(&self) -> BTreeSet<PluginId> {
            self.descriptors
                .iter()
                .filter(|descriptor| descriptor.default_enabled)
                .map(|descriptor| descriptor.id)
                .collect()
        }

        #[must_use]
        pub fn plugin_for_path(&self, path: &str) -> PluginId {
            self.descriptors
                .iter()
                .find(|descriptor| {
                    descriptor
                        .routes
                        .iter()
                        .any(|prefix| path == prefix || path.starts_with(&format!("{prefix}/")))
                })
                .map_or(PluginId::Core, |descriptor| descriptor.id)
        }

        pub fn validate_enable(&self, id: PluginId, active: &BTreeSet<PluginId>) -> Result<()> {
            let descriptor = self
                .descriptor(id)
                .ok_or(MaviError::NotFound { resource: "plugin" })?;
            if id.is_core() {
                return Err(MaviError::conflict("core_plugin_cannot_be_disabled"));
            }
            for dependency in &descriptor.dependencies {
                if !active.contains(dependency) {
                    return Err(MaviError::conflict(format!(
                        "plugin_dependency_required:{dependency}"
                    )));
                }
            }
            Ok(())
        }

        pub fn validate_disable(&self, id: PluginId, active: &BTreeSet<PluginId>) -> Result<()> {
            if id.is_core() {
                return Err(MaviError::conflict("core_plugin_cannot_be_disabled"));
            }
            if !active.contains(&id) {
                return Err(MaviError::conflict("plugin_already_disabled"));
            }
            if self.descriptors.iter().any(|descriptor| {
                descriptor.dependencies.contains(&id) && active.contains(&descriptor.id)
            }) {
                return Err(MaviError::conflict("plugin_has_active_dependents"));
            }
            Ok(())
        }

        /// Validates a snapshot read from `PostgreSQL` before it can reach a
        /// route gate, Cedar context or worker. The activation API enforces
        /// this invariant for normal writes, but a direct database edit or a
        /// partially restored backup must fail closed instead of exposing a
        /// plugin whose dependency is absent.
        pub fn validate_active(&self, active: &BTreeSet<PluginId>) -> Result<()> {
            if !active.contains(&PluginId::Core) {
                return Err(MaviError::Internal);
            }
            for id in active {
                let descriptor = self.descriptor(*id).ok_or(MaviError::Internal)?;
                if descriptor
                    .dependencies
                    .iter()
                    .any(|dependency| !active.contains(dependency))
                {
                    return Err(MaviError::Internal);
                }
            }
            Ok(())
        }
    }

    fn descriptor(
        id: PluginId,
        default_enabled: bool,
        dependencies: &[PluginId],
        routes: &[&str],
        permissions: &[&str],
        navigation: &[&str],
        workflows: &[&str],
    ) -> PluginDescriptor {
        PluginDescriptor {
            id,
            version: env!("CARGO_PKG_VERSION").to_owned(),
            dependencies: dependencies.to_vec(),
            default_enabled,
            routes: routes.iter().map(|route| (*route).to_owned()).collect(),
            permissions: permissions
                .iter()
                .map(|action| Permission::new(id, *action))
                .collect(),
            navigation: navigation
                .iter()
                .map(|href| NavigationItem {
                    label: href
                        .rsplit('/')
                        .next()
                        .unwrap_or("dashboard")
                        .replace('-', " "),
                    href: (*href).to_owned(),
                })
                .collect(),
            workflows: workflows
                .iter()
                .map(|workflow| (*workflow).to_owned())
                .collect(),
            cedar_policy: format!(
                "permit (principal, action, resource) when {{\n    context.plugin == \"{id}\" &&\n    principal.site_id == resource.site_id &&\n    principal.site_id == context.site_id &&\n    principal.plugins.contains(context.plugin) &&\n    resource.plugins.contains(context.plugin) &&\n    (context.resource_type == \"*\" || context.resource_type == resource.kind) &&\n    (principal.permissions.contains(context.permission) ||\n     resource.permissions.contains(context.permission))\n}};"
            ),
        }
    }

    impl PluginRecord {
        fn from_row(row: &sqlx::postgres::PgRow, descriptor: &PluginDescriptor) -> Result<Self> {
            let plugin_id: String = row.try_get("plugin_id").map_err(|_| MaviError::Internal)?;
            let id = plugin_id.parse().map_err(|()| MaviError::Internal)?;
            if id != descriptor.id {
                return Err(MaviError::Internal);
            }
            Ok(Self {
                id,
                version: descriptor.version.clone(),
                dependencies: descriptor.dependencies.clone(),
                default_enabled: descriptor.default_enabled,
                enabled: row.try_get("enabled").map_err(|_| MaviError::Internal)?,
                config: row.try_get("config").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
            })
        }
    }

    #[derive(Clone, Debug)]
    pub struct PluginService {
        pub registry: PluginRegistry,
        cache: Arc<RwLock<Option<BTreeSet<PluginId>>>>,
    }

    impl Default for PluginService {
        fn default() -> Self {
            Self::new(PluginRegistry::built_in())
        }
    }

    impl PluginService {
        #[must_use]
        pub fn new(registry: PluginRegistry) -> Self {
            Self {
                registry,
                cache: Arc::new(RwLock::new(None)),
            }
        }

        /// Invalidates the process-local snapshot after a `PostgreSQL`
        /// `NOTIFY`. The next request reloads the authoritative site state.
        pub fn invalidate(&self) {
            if let Ok(mut cache) = self.cache.write() {
                *cache = None;
            }
        }

        fn cached_enabled_set(&self) -> Option<BTreeSet<PluginId>> {
            self.cache.read().ok().and_then(|cache| cache.clone())
        }

        fn cache_enabled_set(&self, active: BTreeSet<PluginId>) {
            if let Ok(mut cache) = self.cache.write() {
                *cache = Some(active);
            }
        }

        pub async fn list(&self, transaction: &mut SiteTx) -> Result<Vec<PluginRecord>> {
            let rows = sqlx::query(
                "select plugin_id, enabled, config, updated_at
                 from site_plugins order by plugin_id",
            )
            .fetch_all(transaction.conn())
            .await
            .map_err(|_| MaviError::Internal)?;

            self.registry
                .descriptors()
                .iter()
                .map(|descriptor| {
                    let row = rows
                        .iter()
                        .find(|row| {
                            row.try_get::<String, _>("plugin_id").ok().as_deref()
                                == Some(descriptor.id.as_str())
                        })
                        .ok_or(MaviError::Internal)?;
                    PluginRecord::from_row(row, descriptor)
                })
                .collect()
        }

        pub async fn enabled_set(&self, transaction: &mut SiteTx) -> Result<BTreeSet<PluginId>> {
            if let Some(active) = self.cached_enabled_set() {
                self.registry.validate_active(&active)?;
                return Ok(active);
            }
            let active: BTreeSet<PluginId> = self
                .list(transaction)
                .await?
                .into_iter()
                .filter(|plugin| plugin.enabled)
                .map(|plugin| plugin.id)
                .collect();
            self.registry.validate_active(&active)?;
            self.cache_enabled_set(active.clone());
            Ok(active)
        }

        /// Read the activation snapshot while holding every plugin row lock.
        /// Lifecycle changes must use this path so two concurrent enable or
        /// disable requests cannot validate against the same stale snapshot.
        async fn locked_enabled_set(&self, transaction: &mut SiteTx) -> Result<BTreeSet<PluginId>> {
            let rows = sqlx::query(
                "select plugin_id, enabled
                   from site_plugins
                  order by plugin_id
                  for update",
            )
            .fetch_all(transaction.conn())
            .await
            .map_err(|_| MaviError::Internal)?;

            let active = rows
                .into_iter()
                .try_fold(BTreeSet::new(), |mut active, row| {
                    let raw_id: String =
                        row.try_get("plugin_id").map_err(|_| MaviError::Internal)?;
                    let id = raw_id.parse().map_err(|()| MaviError::Internal)?;
                    if row.try_get("enabled").map_err(|_| MaviError::Internal)? {
                        active.insert(id);
                    }
                    Ok::<_, MaviError>(active)
                })?;
            self.registry.validate_active(&active)?;
            Ok(active)
        }

        /// Plugin lifecycle is an owner operation, not merely a broad people
        /// permission. Keep this check in the application service so custom
        /// roles cannot activate code packages even if they can administer
        /// ordinary people records.
        pub async fn is_owner(
            &self,
            transaction: &mut SiteTx,
            context: &mavi_core::SiteContext,
        ) -> Result<bool> {
            let mavi_core::Caller::Account { person_id, .. } = &context.caller else {
                return Ok(false);
            };
            sqlx::query_scalar(
                "select exists(
                    select 1
                      from person_roles pr
                      join roles r on r.site_id = pr.site_id and r.id = pr.role_id
                     where pr.site_id = $1 and pr.person_id = $2
                       and r.name = 'owner' and r.system_role
                )",
            )
            .bind(context.site_id.into_uuid())
            .bind((*person_id).into_uuid())
            .fetch_one(transaction.conn())
            .await
            .map_err(|_| MaviError::Internal)
        }

        /// Lists plugin state through the application authorization boundary.
        /// The transport layer should not need to know which Cedar resource
        /// represents the site-wide plugin registry.
        pub async fn list_for_context(
            &self,
            transaction: &mut SiteTx,
            context: &mavi_core::SiteContext,
            authorization: &super::AuthorizationService,
        ) -> Result<Vec<PluginRecord>> {
            let active = self.enabled_set(transaction).await?;
            authorization.authorize(
                context,
                &Permission::new(PluginId::Core, "plugins.list"),
                "Site",
                context.site_id.to_string(),
                context.site_id,
                &active,
            )?;
            self.list(transaction).await
        }

        /// Executes the complete plugin lifecycle use-case in one transaction:
        /// owner check, Cedar decision, dependency validation and the
        /// activation update all share the same locked snapshot.
        pub async fn set_enabled_for_context(
            &self,
            transaction: &mut SiteTx,
            context: &mavi_core::SiteContext,
            authorization: &super::AuthorizationService,
            id: PluginId,
            enabled: bool,
        ) -> Result<PluginRecord> {
            let active = self.locked_enabled_set(transaction).await?;
            if !self.is_owner(transaction, context).await? {
                return Err(MaviError::Forbidden);
            }
            let action = if enabled {
                "plugins.activate"
            } else {
                "plugins.deactivate"
            };
            authorization.authorize(
                context,
                &Permission::new(PluginId::Core, action),
                "Plugin",
                id.as_str(),
                context.site_id,
                &active,
            )?;
            if enabled {
                self.registry.validate_enable(id, &active)?;
            } else {
                self.registry.validate_disable(id, &active)?;
            }
            self.apply_enabled(transaction, id, enabled).await
        }

        pub async fn set_enabled(
            &self,
            transaction: &mut SiteTx,
            id: PluginId,
            enabled: bool,
        ) -> Result<PluginRecord> {
            let active = self.locked_enabled_set(transaction).await?;
            if enabled {
                self.registry.validate_enable(id, &active)?;
            } else {
                self.registry.validate_disable(id, &active)?;
            }

            self.apply_enabled(transaction, id, enabled).await
        }

        async fn apply_enabled(
            &self,
            transaction: &mut SiteTx,
            id: PluginId,
            enabled: bool,
        ) -> Result<PluginRecord> {
            sqlx::query(
                "update site_plugins
                 set enabled = $1, updated_at = now()
                 where plugin_id = $2",
            )
            .bind(enabled)
            .bind(id.as_str())
            .execute(transaction.conn())
            .await
            .map_err(|_| MaviError::Internal)?;

            sqlx::query("select pg_notify('mavi_plugin_changed', $1)")
                .bind(id.as_str())
                .execute(transaction.conn())
                .await
                .map_err(|_| MaviError::Internal)?;

            self.invalidate();
            self.list(transaction)
                .await?
                .into_iter()
                .find(|plugin| plugin.id == id)
                .ok_or(MaviError::Internal)
        }
    }
}

pub use jobs::{
    DEFAULT_LEASE_SECONDS, Job, JobClaim, JobKind, JobListFilter, JobState, JobsService,
    LeaseOutcome, WorkflowScheduler, retry_delay,
};
pub use plugins::{NavigationItem, PluginDescriptor, PluginRecord, PluginRegistry, PluginService};

/// The durable unit emitted by a domain mutation. Payloads contain IDs and
/// small control values only; large documents and binaries remain in Mavi's
/// database/filesystem and are loaded by the executor.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkflowIntent {
    pub id: Uuid,
    pub site_id: mavi_core::SiteId,
    pub plugin: PluginId,
    pub workflow: String,
    pub idempotency_key: String,
    pub payload: Value,
}

impl WorkflowIntent {
    pub fn new(
        site_id: mavi_core::SiteId,
        plugin: PluginId,
        workflow: impl Into<String>,
        idempotency_key: impl Into<String>,
        payload: Value,
    ) -> Result<Self> {
        let workflow = workflow.into();
        let idempotency_key = idempotency_key.into();
        let intent = Self {
            id: Uuid::now_v7(),
            site_id,
            plugin,
            workflow,
            idempotency_key,
            payload,
        };
        intent.validate()?;
        Ok(intent)
    }

    pub fn validate(&self) -> Result<()> {
        if self.site_id.into_uuid().is_nil() {
            return Err(MaviError::validation("workflow_site_id_invalid"));
        }
        if self.workflow.trim().is_empty() || self.workflow.chars().count() > 160 {
            return Err(MaviError::validation("workflow_name_invalid"));
        }
        if self.idempotency_key.is_empty() || self.idempotency_key.len() > 200 {
            return Err(MaviError::validation("workflow_idempotency_key_invalid"));
        }
        if !self.payload.is_object() {
            return Err(MaviError::validation("workflow_payload_must_be_object"));
        }
        let payload_bytes = serde_json::to_vec(&self.payload)
            .map_err(|_| MaviError::validation("workflow_payload_invalid"))?;
        if payload_bytes.len() > 64 * 1024 {
            return Err(MaviError::validation("workflow_payload_too_large"));
        }
        Ok(())
    }
}

/// Business execution stays in Rust while Hatchet owns delivery semantics.
/// The bridge passes an owned intent to this port after Hatchet has applied
/// retry, timeout, priority and rate-limit policy.
pub trait WorkflowExecutor: Send + Sync {
    fn execute(&self, intent: WorkflowIntent) -> BoxFuture<'_, Result<()>>;
}

/// Application authorization façade. Legacy grant records are translated to
/// the namespaced `Permission` model at this boundary; callers never ask a
/// domain-owned grant set to make the final authorization decision.
#[derive(Clone, Debug)]
pub struct AuthorizationService {
    cedar: CedarAuthorizer,
    active_plugins: Arc<RwLock<Option<BTreeSet<PluginId>>>>,
    observer: Option<Arc<dyn AuthorizationObserver>>,
}

impl AuthorizationService {
    pub fn new() -> Result<Self> {
        Ok(Self {
            cedar: CedarAuthorizer::new()?,
            active_plugins: Arc::new(RwLock::new(None)),
            observer: None,
        })
    }

    pub fn new_with_plugin_policies(registry: &PluginRegistry) -> Result<Self> {
        Self::new_with_plugin_policies_and_observer(registry, None)
    }

    pub fn new_with_plugin_policies_and_observer(
        registry: &PluginRegistry,
        observer: Option<Arc<dyn AuthorizationObserver>>,
    ) -> Result<Self> {
        let fragments = registry
            .descriptors()
            .iter()
            .map(|descriptor| descriptor.cedar_policy.as_str());
        Ok(Self {
            cedar: CedarAuthorizer::new_with_policy_fragments(fragments)?,
            active_plugins: Arc::new(RwLock::new(None)),
            observer,
        })
    }

    pub fn new_with_observer(observer: Arc<dyn AuthorizationObserver>) -> Result<Self> {
        Ok(Self {
            cedar: CedarAuthorizer::new()?,
            active_plugins: Arc::new(RwLock::new(None)),
            observer: Some(observer),
        })
    }

    fn observe(&self, result: Result<()>) -> Result<()> {
        if let Some(observer) = &self.observer {
            let outcome = match &result {
                Ok(()) => AuthorizationOutcome::Allowed,
                Err(MaviError::Internal) => AuthorizationOutcome::EvaluationError,
                Err(_) => AuthorizationOutcome::Denied,
            };
            observer.record_authorization(outcome);
        }
        result
    }

    /// Publishes the site activation snapshot loaded by the HTTP plugin gate.
    /// Legacy grant helpers use this cache only as a compatibility boundary;
    /// new application use-cases pass the active set explicitly to Cedar.
    pub fn set_active_plugins(&self, active_plugins: BTreeSet<PluginId>) {
        if let Ok(mut snapshot) = self.active_plugins.write() {
            *snapshot = Some(active_plugins);
        }
    }

    pub fn authorize(
        &self,
        context: &SiteContext,
        permission: &Permission,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        resource_site_id: mavi_core::SiteId,
        active_plugins: &BTreeSet<PluginId>,
    ) -> Result<()> {
        self.observe(self.cedar.authorize_permission(
            context,
            permission,
            resource_type,
            resource_id,
            resource_site_id,
            active_plugins,
        ))
    }

    /// Authorizes a business permission using the activation snapshot loaded
    /// by the HTTP plugin gate. This is the application boundary for
    /// handlers that already have a typed action and do not need a legacy
    /// `Grant` translation.
    pub fn authorize_cached(
        &self,
        context: &SiteContext,
        permission: &Permission,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        resource_site_id: mavi_core::SiteId,
    ) -> Result<()> {
        let active_plugins = match self.active_plugins.read() {
            Ok(snapshot) => match snapshot.clone() {
                Some(active_plugins) => active_plugins,
                None => return self.observe(Err(MaviError::Forbidden)),
            },
            Err(_) => return self.observe(Err(MaviError::Internal)),
        };
        self.authorize(
            context,
            permission,
            resource_type,
            resource_id,
            resource_site_id,
            &active_plugins,
        )
    }

    // Keep the application authorization port aligned with the Cedar port so
    // resource/site scope cannot be omitted by a caller.
    #[allow(clippy::too_many_arguments)]
    pub fn authorize_with_resource_grants(
        &self,
        context: &SiteContext,
        permission: &Permission,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        resource_site_id: mavi_core::SiteId,
        active_plugins: &BTreeSet<PluginId>,
        resource_grants: &Grants,
    ) -> Result<()> {
        self.observe(self.cedar.authorize_permission_with_resource_grants(
            context,
            permission,
            resource_type,
            resource_id,
            resource_site_id,
            active_plugins,
            resource_grants,
        ))
    }

    pub fn authorize_grant(
        &self,
        context: &SiteContext,
        grant: Grant,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        resource_site_id: SiteId,
        resource_grants: &Grants,
    ) -> Result<()> {
        let active_plugins = match self.active_plugins.read() {
            Ok(snapshot) => match snapshot.clone() {
                Some(active_plugins) => active_plugins,
                None => return self.observe(Err(MaviError::Forbidden)),
            },
            Err(_) => return self.observe(Err(MaviError::Internal)),
        };
        // Existing domain handlers still expose the legacy Capability/Action
        // pair. The Cedar adapter translates that pair to the namespaced
        // business-action catalog before evaluating it.
        if !active_plugins.contains(&grant.capability.plugin()) {
            return self.observe(Err(MaviError::Forbidden));
        }
        self.observe(self.cedar.authorize_legacy_grant(
            context,
            grant,
            resource_type,
            resource_id,
            resource_site_id,
            &active_plugins,
            resource_grants,
        ))
    }
}

/// Small private client for the Go Hatchet adapter. Rust never receives a
/// Hatchet SDK dependency or a browser-visible token; it sends an outbox
/// intent over the deployment's private bridge boundary.
#[derive(Clone, Debug)]
pub struct HatchetBridgeClient {
    client: reqwest::Client,
    base_url: String,
    namespace: String,
    secret: String,
}

impl HatchetBridgeClient {
    pub fn required_from_env() -> Result<Self> {
        Self::from_env()?.ok_or_else(|| MaviError::validation("mavi_hatchet_bridge_url_required"))
    }

    pub fn from_env() -> Result<Option<Self>> {
        let Some(base_url) = std::env::var_os("MAVI_HATCHET_BRIDGE_URL") else {
            return Ok(None);
        };
        let base_url = base_url
            .to_str()
            .ok_or_else(|| MaviError::validation("invalid_mavi_hatchet_bridge_url"))?
            .trim_end_matches('/')
            .to_owned();
        if base_url.is_empty() {
            return Err(MaviError::validation("invalid_mavi_hatchet_bridge_url"));
        }
        let secret = std::env::var("MAVI_HATCHET_BRIDGE_SECRET")
            .map_err(|_| MaviError::validation("mavi_hatchet_bridge_secret_required"))?;
        if secret.is_empty() {
            return Err(MaviError::validation("mavi_hatchet_bridge_secret_required"));
        }
        Ok(Some(Self {
            client: reqwest::Client::new(),
            base_url,
            namespace: std::env::var("MAVI_HATCHET_NAMESPACE")
                .unwrap_or_else(|_| "mavi".to_owned()),
            secret,
        }))
    }

    pub async fn publish(&self, intent: &WorkflowIntent) -> Result<String> {
        let response = self
            .client
            .post(format!("{}/internal/v1/workflows/dispatch", self.base_url))
            .header("authorization", format!("Bearer {}", self.secret))
            .header("x-mavi-hatchet-namespace", &self.namespace)
            .json(intent)
            .send()
            .await
            .map_err(|_| MaviError::Internal)?;
        if !response.status().is_success() {
            return Err(MaviError::Internal);
        }
        let body = response
            .json::<BridgeDispatchResponse>()
            .await
            .map_err(|_| MaviError::Internal)?;
        Ok(body.run_id.unwrap_or_else(|| intent.id.to_string()))
    }

    /// Ask Hatchet to cancel the already-dispatched run. The local run state
    /// is updated by the application transaction only after this private
    /// control-plane request succeeds.
    pub async fn cancel_run(&self, run_id: &str) -> Result<Option<String>> {
        self.control_run("cancel", run_id).await
    }

    pub async fn replay_run(&self, run_id: &str) -> Result<Option<String>> {
        self.control_run("replay", run_id).await
    }

    async fn control_run(&self, operation: &str, run_id: &str) -> Result<Option<String>> {
        let response = self
            .client
            .post(format!(
                "{}/internal/v1/workflows/{operation}",
                self.base_url
            ))
            .header("authorization", format!("Bearer {}", self.secret))
            .header("x-mavi-hatchet-namespace", &self.namespace)
            .json(&serde_json::json!({"run_id": run_id}))
            .send()
            .await
            .map_err(|_| MaviError::Internal)?;
        if response.status().is_success() {
            let body = response.bytes().await.map_err(|_| MaviError::Internal)?;
            if body.is_empty() {
                return Ok(None);
            }
            let body = serde_json::from_slice::<BridgeControlResponse>(&body)
                .map_err(|_| MaviError::Internal)?;
            Ok(body.run_id)
        } else {
            Err(MaviError::Internal)
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct BridgeDispatchResponse {
    run_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct BridgeControlResponse {
    run_id: Option<String>,
}

/// Transactional workflow intent persistence and outbox state transitions.
#[derive(Clone, Debug, Default)]
pub struct WorkflowService;

#[derive(Clone, Debug, Serialize)]
pub struct WorkflowRunRecord {
    pub id: String,
    pub site_id: mavi_core::SiteId,
    pub plugin: PluginId,
    pub workflow: String,
    pub hatchet_run_id: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowRunListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct WorkflowRunCursor {
    updated_at: DateTime<Utc>,
    idempotency_key: String,
}

impl WorkflowService {
    pub async fn enqueue(&self, transaction: &mut SiteTx, intent: &WorkflowIntent) -> Result<()> {
        intent.validate()?;
        if transaction.site_id() != intent.site_id {
            return Err(MaviError::Forbidden);
        }
        let inserted = sqlx::query(
            "insert into workflow_outbox
                (id, idempotency_key, site_id, plugin_id, workflow, payload)
             values ($1, $2, $3, $4, $5, $6)
             on conflict (site_id, idempotency_key) do nothing
             returning idempotency_key",
        )
        .bind(intent.id)
        .bind(&intent.idempotency_key)
        .bind(intent.site_id.into_uuid())
        .bind(intent.plugin.as_str())
        .bind(&intent.workflow)
        .bind(&intent.payload)
        .fetch_optional(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if inserted.is_none() {
            let existing = sqlx::query(
                "select site_id, plugin_id, workflow, payload
                   from workflow_outbox
                  where site_id = $1 and idempotency_key = $2",
            )
            .bind(intent.site_id.into_uuid())
            .bind(&intent.idempotency_key)
            .fetch_optional(transaction.conn())
            .await
            .map_err(|_| MaviError::Internal)?
            .ok_or(MaviError::Internal)?;
            let existing_site_id: uuid::Uuid = existing
                .try_get("site_id")
                .map_err(|_| MaviError::Internal)?;
            let existing_plugin: String = existing
                .try_get("plugin_id")
                .map_err(|_| MaviError::Internal)?;
            let existing_workflow: String = existing
                .try_get("workflow")
                .map_err(|_| MaviError::Internal)?;
            let existing_payload: Value = existing
                .try_get("payload")
                .map_err(|_| MaviError::Internal)?;
            if existing_site_id != intent.site_id.into_uuid()
                || existing_plugin != intent.plugin.as_str()
                || existing_workflow != intent.workflow
                || existing_payload != intent.payload
            {
                return Err(MaviError::conflict("workflow_idempotency_payload_mismatch"));
            }
        }
        sqlx::query(
            "insert into workflow_runs
                (idempotency_key, site_id, plugin_id, workflow)
             values ($1, $2, $3, $4)
             on conflict (site_id, idempotency_key) do nothing",
        )
        .bind(&intent.idempotency_key)
        .bind(intent.site_id.into_uuid())
        .bind(intent.plugin.as_str())
        .bind(&intent.workflow)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        Ok(())
    }

    pub async fn claim(&self, transaction: &mut SiteTx) -> Result<Option<WorkflowIntent>> {
        let row = sqlx::query(
            "select id, idempotency_key, site_id, plugin_id, workflow, payload
             from workflow_outbox
             where status in ('pending', 'failed', 'publishing')
               and available_at <= now()
               and not exists (
                   select 1 from workflow_runs run
                    where run.site_id = workflow_outbox.site_id
                      and run.idempotency_key = workflow_outbox.idempotency_key
                      and run.status in ('cancelled', 'paused')
               )
             order by created_at
             for update skip locked limit 1",
        )
        .fetch_optional(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let idempotency_key: String = row
            .try_get("idempotency_key")
            .map_err(|_| MaviError::Internal)?;
        sqlx::query(
            "update workflow_outbox
             set status = 'publishing', attempts = attempts + 1,
                 available_at = now() + interval '30 seconds'
             where site_id = $1 and idempotency_key = $2",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(&idempotency_key)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let plugin_id: String = row.try_get("plugin_id").map_err(|_| MaviError::Internal)?;
        let plugin = plugin_id.parse().map_err(|()| MaviError::Internal)?;
        Ok(Some(WorkflowIntent {
            id: row.try_get("id").map_err(|_| MaviError::Internal)?,
            site_id: mavi_core::SiteId::from_uuid(
                row.try_get("site_id").map_err(|_| MaviError::Internal)?,
            ),
            plugin,
            workflow: row.try_get("workflow").map_err(|_| MaviError::Internal)?,
            idempotency_key,
            payload: row.try_get("payload").map_err(|_| MaviError::Internal)?,
        }))
    }

    pub async fn mark_published(
        &self,
        transaction: &mut SiteTx,
        intent: &WorkflowIntent,
        run_id: &str,
    ) -> Result<()> {
        sqlx::query(
            "update workflow_outbox
             set status = 'published', published_at = now(), last_error = null
             where site_id = $1 and idempotency_key = $2
               and status not in ('cancelled', 'paused')",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(&intent.idempotency_key)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        sqlx::query(
            "update workflow_runs
             set status = case when status = 'pending' then 'running' else status end,
                 hatchet_run_id = $3, updated_at = now()
             where site_id = $1 and idempotency_key = $2
               and status not in ('completed', 'cancelled', 'paused')",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(&intent.idempotency_key)
        .bind(run_id)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        Ok(())
    }

    pub async fn mark_failed(
        &self,
        transaction: &mut SiteTx,
        intent: &WorkflowIntent,
        error: &str,
    ) -> Result<()> {
        sqlx::query(
            "update workflow_outbox
             set status = 'failed', last_error = $3,
                 available_at = now() + interval '30 seconds'
             where site_id = $1 and idempotency_key = $2
               and status not in ('completed', 'cancelled', 'paused')",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(&intent.idempotency_key)
        .bind(error)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        Ok(())
    }

    pub async fn list_runs(
        &self,
        transaction: &mut SiteTx,
        filter: &WorkflowRunListFilter,
    ) -> Result<Page<WorkflowRunRecord>> {
        let after = filter
            .page
            .after
            .as_ref()
            .map(decode_workflow_cursor)
            .transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = QueryBuilder::<Postgres>::new(
            "select idempotency_key, site_id, plugin_id, workflow, hatchet_run_id,
                    status, created_at, updated_at
               from workflow_runs
              where site_id = ",
        );
        query.push_bind(transaction.site_id().into_uuid());
        if let Some(after) = after {
            query
                .push(" and (updated_at, idempotency_key) < (")
                .push_bind(after.updated_at)
                .push(", ")
                .push_bind(after.idempotency_key)
                .push(")");
        }
        query
            .push(" order by updated_at desc, idempotency_key desc limit ")
            .push_bind(limit + 1);
        let rows = query
            .build()
            .fetch_all(transaction.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows.iter().map(run_from_row).collect::<Result<Vec<_>>>()?;
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let last = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_workflow_cursor(last.updated_at, &last.id)?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn get_run(&self, transaction: &mut SiteTx, id: &str) -> Result<WorkflowRunRecord> {
        let row = sqlx::query(
            "select idempotency_key, site_id, plugin_id, workflow, hatchet_run_id,
                    status, created_at, updated_at
             from workflow_runs where site_id = $1 and idempotency_key = $2",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(id)
        .fetch_optional(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: "workflow_run",
        })?;
        run_from_row(&row)
    }

    pub async fn cancel(&self, transaction: &mut SiteTx, id: &str) -> Result<WorkflowRunRecord> {
        let result = sqlx::query(
            "update workflow_runs
                set status = 'cancelled', claim_token = null, claim_worker = null,
                    claim_until = null, updated_at = now()
             where site_id = $1 and idempotency_key = $2
               and status not in ('completed', 'cancelled')",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(id)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if result.rows_affected() == 0 {
            let run = self.get_run(transaction, id).await?;
            if run.status != "cancelled" {
                return Err(MaviError::conflict("workflow_run_not_cancellable"));
            }
            return Ok(run);
        }
        sqlx::query(
            "update workflow_outbox
                set status = case when status in ('pending', 'failed', 'publishing')
                                  then 'cancelled' else status end
              where site_id = $1 and idempotency_key = $2",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(id)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        self.get_run(transaction, id).await
    }

    /// Pauses a run at the Mavi boundary. Pending relay work is withheld;
    /// already-published work is cancelled by the HTTP/bridge layer before
    /// this transaction is committed. Resume creates a fresh Hatchet
    /// delivery with the same idempotency key.
    pub async fn pause(&self, transaction: &mut SiteTx, id: &str) -> Result<WorkflowRunRecord> {
        let result = sqlx::query(
            "update workflow_runs
                set status = 'paused', claim_token = null, claim_worker = null,
                    claim_until = null, updated_at = now()
             where site_id = $1 and idempotency_key = $2
               and status not in ('completed', 'cancelled', 'paused')",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(id)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if result.rows_affected() == 0 {
            let run = self.get_run(transaction, id).await?;
            if run.status == "paused" {
                return Ok(run);
            }
            return Err(MaviError::conflict("workflow_run_not_pausable"));
        }
        sqlx::query(
            "update workflow_outbox set status = 'paused'
             where site_id = $1 and idempotency_key = $2
               and status not in ('cancelled', 'paused')",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(id)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        sqlx::query("select pg_notify('mavi_workflow_changed', $1)")
            .bind(id)
            .execute(transaction.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        self.get_run(transaction, id).await
    }

    /// Reopens a paused workflow for relay delivery. The same intent and
    /// idempotency key are reused, so a duplicate resume cannot create two
    /// local workflow records.
    pub async fn resume(&self, transaction: &mut SiteTx, id: &str) -> Result<WorkflowRunRecord> {
        let result = sqlx::query(
            "update workflow_runs
                set status = 'pending', claim_token = null, claim_worker = null,
                    claim_until = null, updated_at = now()
             where site_id = $1 and idempotency_key = $2 and status = 'paused'",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(id)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if result.rows_affected() == 0 {
            let run = self.get_run(transaction, id).await?;
            if run.status != "paused" {
                return Err(MaviError::conflict("workflow_run_not_resumable"));
            }
            return Ok(run);
        }
        sqlx::query(
            "update workflow_outbox set status = 'pending', available_at = now(), last_error = null
             where site_id = $1 and idempotency_key = $2 and status = 'paused'",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(id)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        sqlx::query("select pg_notify('mavi_workflow_changed', $1)")
            .bind(id)
            .execute(transaction.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        self.get_run(transaction, id).await
    }

    pub async fn replay(&self, transaction: &mut SiteTx, id: &str) -> Result<WorkflowRunRecord> {
        let result = sqlx::query(
            "update workflow_outbox
             set status = 'pending', available_at = now(), last_error = null
             where site_id = $1 and idempotency_key = $2",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(id)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if result.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: "workflow_run",
            });
        }
        sqlx::query(
            "update workflow_runs
                set status = 'pending', attempts = 0, claim_token = null,
                    claim_worker = null, claim_until = null, updated_at = now()
             where site_id = $1 and idempotency_key = $2",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(id)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        sqlx::query("select pg_notify('mavi_workflow_changed', $1)")
            .bind(id)
            .execute(transaction.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        self.get_run(transaction, id).await
    }

    /// Mark a Hatchet-native replay as running without creating a second
    /// outbox delivery. This is used only after the bridge has accepted the
    /// replay request; a run that has never reached Hatchet still uses the
    /// durable outbox replay path above.
    pub async fn mark_replayed(
        &self,
        transaction: &mut SiteTx,
        id: &str,
        hatchet_run_id: Option<&str>,
    ) -> Result<WorkflowRunRecord> {
        sqlx::query(
            "update workflow_outbox
                set status = 'published', published_at = coalesce(published_at, now()),
                    last_error = null
             where site_id = $1 and idempotency_key = $2",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(id)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let result = sqlx::query(
            "update workflow_runs
                set status = 'running', claim_token = null, claim_worker = null,
                    claim_until = null, hatchet_run_id = coalesce($3, hatchet_run_id),
                    updated_at = now()
             where site_id = $1 and idempotency_key = $2 and hatchet_run_id is not null",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(id)
        .bind(hatchet_run_id)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if result.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: "workflow_run",
            });
        }
        self.get_run(transaction, id).await
    }

    pub async fn set_hatchet_run_id(
        &self,
        transaction: &mut SiteTx,
        id: &str,
        hatchet_run_id: Option<&str>,
    ) -> Result<()> {
        let result = sqlx::query(
            "update workflow_runs
                set hatchet_run_id = $3, updated_at = now()
             where site_id = $1 and idempotency_key = $2",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(id)
        .bind(hatchet_run_id)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if result.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: "workflow_run",
            });
        }
        Ok(())
    }

    pub async fn mark_completed(
        &self,
        transaction: &mut SiteTx,
        id: &str,
    ) -> Result<WorkflowRunRecord> {
        let result = sqlx::query(
            "update workflow_runs
                 set status = 'completed', claim_token = null,
                     claim_worker = null, claim_until = null, updated_at = now()
              where site_id = $1 and idempotency_key = $2
                and status not in ('cancelled', 'completed')",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(id)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if result.rows_affected() == 0 {
            let run = self.get_run(transaction, id).await?;
            if matches!(run.status.as_str(), "completed" | "cancelled" | "paused") {
                return Ok(run);
            }
            return Err(MaviError::conflict("workflow_run_not_completable"));
        }
        self.get_run(transaction, id).await
    }

    /// Records the latest failed Hatchet attempt without closing the outbox
    /// permanently. A subsequent Hatchet retry may move the same run back to
    /// `completed`; if retries are exhausted, the run remains visibly failed
    /// in the application API.
    pub async fn mark_run_failed(
        &self,
        transaction: &mut SiteTx,
        id: &str,
    ) -> Result<WorkflowRunRecord> {
        let result = sqlx::query(
            "update workflow_runs
                 set status = 'failed', claim_token = null,
                     claim_worker = null, claim_until = null, updated_at = now()
             where site_id = $1 and idempotency_key = $2
               and status not in ('cancelled', 'completed')",
        )
        .bind(transaction.site_id().into_uuid())
        .bind(id)
        .execute(transaction.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if result.rows_affected() == 0 {
            let run = self.get_run(transaction, id).await?;
            if matches!(
                run.status.as_str(),
                "completed" | "failed" | "cancelled" | "paused"
            ) {
                return Ok(run);
            }
            return Err(MaviError::conflict("workflow_run_not_markable_failed"));
        }
        self.get_run(transaction, id).await
    }
}

fn run_from_row(row: &sqlx::postgres::PgRow) -> Result<WorkflowRunRecord> {
    let plugin_id: String = row.try_get("plugin_id").map_err(|_| MaviError::Internal)?;
    Ok(WorkflowRunRecord {
        id: row
            .try_get("idempotency_key")
            .map_err(|_| MaviError::Internal)?,
        site_id: mavi_core::SiteId::from_uuid(
            row.try_get("site_id").map_err(|_| MaviError::Internal)?,
        ),
        plugin: plugin_id.parse().map_err(|()| MaviError::Internal)?,
        workflow: row.try_get("workflow").map_err(|_| MaviError::Internal)?,
        hatchet_run_id: row
            .try_get("hatchet_run_id")
            .map_err(|_| MaviError::Internal)?,
        status: row.try_get("status").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn encode_workflow_cursor(updated_at: DateTime<Utc>, idempotency_key: &str) -> Result<Cursor> {
    let bytes = serde_json::to_vec(&WorkflowRunCursor {
        updated_at,
        idempotency_key: idempotency_key.to_owned(),
    })
    .map_err(|_| MaviError::Internal)?;
    Cursor::parse(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_workflow_cursor(cursor: &Cursor) -> Result<WorkflowRunCursor> {
    let bytes = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        cursor.as_str(),
    )
    .map_err(|_| MaviError::validation("invalid_cursor"))?;
    let cursor: WorkflowRunCursor =
        serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))?;
    if cursor.idempotency_key.is_empty() || cursor.idempotency_key.len() > 200 {
        return Err(MaviError::validation("invalid_cursor"));
    }
    Ok(cursor)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use mavi_core::{Caller, PersonId, RequestId};

    use super::*;

    #[derive(Debug, Default)]
    struct RecordingAuthorizationObserver {
        allowed: AtomicU64,
        denied: AtomicU64,
        errors: AtomicU64,
    }

    impl AuthorizationObserver for RecordingAuthorizationObserver {
        fn record_authorization(&self, outcome: AuthorizationOutcome) {
            match outcome {
                AuthorizationOutcome::Allowed => {
                    self.allowed.fetch_add(1, Ordering::Relaxed);
                }
                AuthorizationOutcome::Denied => {
                    self.denied.fetch_add(1, Ordering::Relaxed);
                }
                AuthorizationOutcome::EvaluationError => {
                    self.errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    #[test]
    fn every_compiled_plugin_contributes_a_valid_cedar_fragment() {
        let registry = PluginRegistry::built_in();
        assert_eq!(registry.descriptors().len(), PluginId::ALL.len());
        assert!(
            registry
                .descriptors()
                .iter()
                .all(|descriptor| !descriptor.cedar_policy.is_empty())
        );
        AuthorizationService::new_with_plugin_policies(&registry).expect("compiled Cedar");
    }

    #[test]
    fn fresh_registry_enables_only_core_and_writing() {
        let registry = PluginRegistry::built_in();
        assert_eq!(
            registry.default_enabled(),
            [PluginId::Core, PluginId::Writing].into_iter().collect()
        );
    }

    #[test]
    fn registry_owns_route_and_dependency_decisions() {
        let registry = PluginRegistry::built_in();
        assert_eq!(
            registry.plugin_for_path("/api/v1/shop/products"),
            PluginId::Commerce
        );
        assert_eq!(
            registry.plugin_for_path("/api/v1/languages"),
            PluginId::Core
        );
        assert!(
            registry
                .validate_enable(PluginId::Commerce, &registry.default_enabled())
                .is_ok()
        );
        assert!(
            registry
                .validate_disable(PluginId::Core, &registry.default_enabled())
                .is_err()
        );
        assert!(
            registry
                .validate_active(&registry.default_enabled())
                .is_ok()
        );
        assert!(
            registry
                .validate_active(&[PluginId::Writing].into_iter().collect())
                .is_err()
        );
        assert!(
            registry
                .validate_active(&[PluginId::Commerce].into_iter().collect())
                .is_err()
        );
    }

    #[test]
    fn compiled_permission_keys_round_trip_through_storage_and_legacy_bridge() {
        let registry = PluginRegistry::built_in();
        for descriptor in registry.descriptors() {
            for permission in &descriptor.permissions {
                let parsed = Permission::from_key(&permission.capability_key())
                    .expect("compiled permission must be parseable from storage");
                assert_eq!(parsed, *permission);
            }
        }

        let audit = Permission::new(PluginId::Governance, "audit.view");
        assert_eq!(
            audit.to_legacy(),
            Some(Grant::new(
                mavi_core::Capability::Audit,
                mavi_core::Action::View
            ))
        );
    }

    #[test]
    fn authorization_observer_receives_low_cardinality_outcomes() {
        let registry = PluginRegistry::built_in();
        let observer = Arc::new(RecordingAuthorizationObserver::default());
        let authorization = AuthorizationService::new_with_plugin_policies_and_observer(
            &registry,
            Some(observer.clone()),
        )
        .expect("compiled Cedar");
        let site_id = SiteId::from_uuid(Uuid::now_v7());
        let context = SiteContext::with_caller(
            site_id,
            Caller::Account {
                person_id: PersonId::from_uuid(Uuid::now_v7()),
                session_id: None,
                grants: Grants::from_permissions([Permission::new(PluginId::Core, "plugins.list")]),
            },
            RequestId::new(),
        );
        let active_plugins = [PluginId::Core].into_iter().collect();

        authorization
            .authorize(
                &context,
                &Permission::new(PluginId::Core, "plugins.list"),
                "Plugin",
                "plugins",
                site_id,
                &active_plugins,
            )
            .expect("held permission should be allowed");
        assert!(
            authorization
                .authorize(
                    &context,
                    &Permission::new(PluginId::Core, "plugins.activate"),
                    "Plugin",
                    "plugins",
                    site_id,
                    &active_plugins,
                )
                .is_err()
        );

        assert_eq!(observer.allowed.load(Ordering::Relaxed), 1);
        assert_eq!(observer.denied.load(Ordering::Relaxed), 1);
        assert_eq!(observer.errors.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn workflow_run_cursor_round_trips_and_rejects_unbounded_keys() {
        let timestamp = Utc::now();
        let cursor = encode_workflow_cursor(timestamp, "automation:run:1").expect("cursor");
        let decoded = decode_workflow_cursor(&cursor).expect("decoded cursor");
        assert_eq!(decoded.updated_at, timestamp);
        assert_eq!(decoded.idempotency_key, "automation:run:1");

        let oversized = encode_workflow_cursor(timestamp, &"x".repeat(201)).expect("cursor");
        assert!(decode_workflow_cursor(&oversized).is_err());
    }
}
