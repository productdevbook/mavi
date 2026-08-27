//! `PostgreSQL` access that cannot forget the current site scope.
//!
//! Domain code receives a [`SiteTx`] rather than a pool. The pool is private,
//! and the transaction sets the `PostgreSQL` scope with `SET LOCAL`, so a
//! connection returned to the pool cannot carry one request's site into the
//! next request.

use mavi_core::{MaviError, PluginId, Result, SiteContext, SiteId};
use sqlx::postgres::{PgConnectOptions, PgListener, PgPoolOptions};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// The highest migration applied by this workspace.
///
/// It is part of the runtime compatibility contract exposed to the operator.
/// Keep it next to the migration runner so a release cannot advertise a
/// storage version independently from the migrations it ships.
pub const CURRENT_SCHEMA_VERSION: u32 = 57;

#[derive(Clone, Debug)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn connect(url: &str, max_connections: u32) -> Result<Self> {
        let options: PgConnectOptions = url.parse().map_err(|_| MaviError::Internal)?;
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect_with(options)
            .await
            .map_err(|_| MaviError::Internal)?;

        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<()> {
        // Migration 0053 backfills the canonical permission projection for
        // legacy role grants. System-role rows are normally immutable, so
        // their protection trigger has to be bypassed for this one-time
        // compatibility transition. Keep the old migration file immutable:
        // this guard also works for databases that already ran earlier
        // migrations and therefore cannot safely receive a checksum change.
        let legacy_permission_backfill = self.prepare_legacy_permission_backfill().await?;
        let migration_result = sqlx::migrate!("./migrations").run(&self.pool).await;

        if legacy_permission_backfill {
            sqlx::query(
                "alter table role_grants
                 enable trigger role_grants_system_role_protected",
            )
            .execute(&self.pool)
            .await
            .map_err(|_| MaviError::Internal)?;
        }

        migration_result.map_err(|_| MaviError::Internal)
    }

    async fn prepare_legacy_permission_backfill(&self) -> Result<bool> {
        let migrations_exist: bool =
            sqlx::query_scalar("select to_regclass('public._sqlx_migrations') is not null")
                .fetch_one(&self.pool)
                .await
                .map_err(|_| MaviError::Internal)?;
        if !migrations_exist {
            return Ok(false);
        }

        let needs_backfill: bool = sqlx::query_scalar(
            "select exists (
                 select 1
                   from _sqlx_migrations
                  where version = 52 and success
             )
             and not exists (
                 select 1
                   from _sqlx_migrations
                  where version = 53 and success
             )
             and exists (select 1 from role_grants)
             and exists (
                 select 1
                   from pg_trigger
                  where tgrelid = 'public.role_grants'::regclass
                    and tgname = 'role_grants_system_role_protected'
                    and not tgisinternal
             )",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|_| MaviError::Internal)?;
        if !needs_backfill {
            return Ok(false);
        }

        sqlx::query(
            "alter table role_grants
             disable trigger role_grants_system_role_protected",
        )
        .execute(&self.pool)
        .await
        .map_err(|_| MaviError::Internal)?;

        Ok(true)
    }

    /// Checks the database connection used by runtime readiness probes.
    ///
    /// This intentionally does not open a site-scoped transaction: readiness
    /// is a process concern, not a request for one site's data.
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("select 1")
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|_| MaviError::Internal)
    }

    /// Opens a dedicated `PostgreSQL` notification connection. Listeners are
    /// intentionally not borrowed from request transactions: a long-lived
    /// `LISTEN` connection must never hold a site mutation transaction open.
    pub async fn listen(&self, channel: &str) -> Result<PgListener> {
        let mut listener = PgListener::connect_with(&self.pool)
            .await
            .map_err(|_| MaviError::Internal)?;
        listener
            .listen(channel)
            .await
            .map_err(|_| MaviError::Internal)?;
        Ok(listener)
    }

    /// Creates the one configured site and seeds the compiled plugin registry.
    pub async fn ensure_site(&self, site_id: SiteId) -> Result<()> {
        self.assert_single_site(site_id).await?;
        self.ensure_site_row(site_id).await
    }

    /// Creates a site for legacy PostgreSQL integration fixtures that need to
    /// exercise RLS across multiple sites. Test fixtures also opt every
    /// compiled plugin in so older domain acceptance tests can exercise their
    /// worker paths explicitly; fresh runtime startup uses [`Self::ensure_site`]
    /// and enables only core and writing.
    ///
    /// This is deliberately not used by runtime startup. It is available only
    /// in debug builds and requires `TEST_DATABASE_URL`, so a production build
    /// cannot accidentally bypass the single-site invariant.
    #[doc(hidden)]
    pub async fn ensure_site_for_tests(&self, site_id: SiteId) -> Result<()> {
        if !cfg!(debug_assertions) || std::env::var_os("TEST_DATABASE_URL").is_none() {
            return Err(MaviError::validation(
                "test_site_helper_requires_test_database",
            ));
        }

        sqlx::query("drop index if exists site_catalog_single_instance")
            .execute(&self.pool)
            .await
            .map_err(|_| MaviError::Internal)?;
        self.ensure_site_row(site_id).await?;
        let context = SiteContext::public(site_id);
        let mut transaction = self.begin(&context).await?;
        sqlx::query("update site_plugins set enabled = true, updated_at = now()")
            .execute(transaction.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        transaction.commit().await
    }

    async fn ensure_site_row(&self, site_id: SiteId) -> Result<()> {
        let mut transaction = self.pool.begin().await.map_err(|_| MaviError::Internal)?;
        sqlx::query(
            "insert into site_catalog (site_id)
             values ($1)
             on conflict (site_id) do nothing",
        )
        .bind(site_id.into_uuid())
        .execute(&mut *transaction)
        .await
        .map_err(|_| MaviError::Internal)?;
        sqlx::query("select set_config('app.site_id', $1, true)")
            .bind(site_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(|_| MaviError::Internal)?;
        for plugin_id in PluginId::ALL {
            let enabled = PluginId::DEFAULT_ENABLED.contains(&plugin_id);
            sqlx::query(
                "insert into site_plugins (site_id, plugin_id, enabled)
                 values ($1, $2, $3)
                 on conflict (site_id, plugin_id) do nothing",
            )
            .bind(site_id.into_uuid())
            .bind(plugin_id.as_str())
            .bind(enabled)
            .execute(&mut *transaction)
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        transaction.commit().await.map_err(|_| MaviError::Internal)
    }

    /// Refuses a database that still contains another site. This is the
    /// single-site runtime preflight; tenant routing/provisioning belongs to
    /// the external control-plane repository.
    pub async fn assert_single_site(&self, site_id: SiteId) -> Result<()> {
        let existing: Option<Uuid> =
            sqlx::query_scalar("select site_id from site_catalog where site_id <> $1 limit 1")
                .bind(site_id.into_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| MaviError::Internal)?;
        if existing.is_some() {
            return Err(MaviError::validation(
                "mavi_single_site_invariant_multiple_sites",
            ));
        }
        Ok(())
    }

    pub async fn begin(&self, context: &SiteContext) -> Result<SiteTx> {
        let mut transaction = self.pool.begin().await.map_err(|_| MaviError::Internal)?;
        sqlx::query("select set_config('app.site_id', $1, true)")
            .bind(context.site_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(|_| MaviError::Internal)?;

        Ok(SiteTx {
            transaction,
            site_id: context.site_id,
        })
    }
}

#[derive(Debug)]
pub struct SiteTx {
    transaction: Transaction<'static, Postgres>,
    site_id: SiteId,
}

impl SiteTx {
    #[must_use]
    pub const fn site_id(&self) -> SiteId {
        self.site_id
    }

    #[must_use]
    pub fn conn(&mut self) -> &mut sqlx::PgConnection {
        &mut self.transaction
    }

    pub async fn commit(self) -> Result<()> {
        self.transaction
            .commit()
            .await
            .map_err(|_| MaviError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use crate::CURRENT_SCHEMA_VERSION;
    use mavi_core::Capability;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn content_schema_is_site_scoped_and_has_composite_revision_links() {
        let migration = include_str!("../migrations/0002_content.sql");

        assert!(migration.contains("primary key (site_id, id)"));
        assert!(migration.contains("alter table content_entries force row level security"));
        assert!(migration.contains("using (site_id = current_setting('app.site_id', true)::uuid)"));
        assert!(
            migration.contains(
                "foreign key (site_id, content_id) references content_entries(site_id, id)"
            )
        );
        assert!(migration.contains("content_entries_site_language_slug"));

        let audit_migration = include_str!("../migrations/0004_audit.sql");
        assert!(audit_migration.contains("alter table audit_events force row level security"));
        assert!(audit_migration.contains("request_id uuid not null"));

        let settings_migration = include_str!("../migrations/0005_settings_languages.sql");
        assert!(settings_migration.contains("primary key (site_id, tag)"));
        assert!(settings_migration.contains("site_languages_one_default"));
        assert!(settings_migration.contains("alter table site_languages force row level security"));

        let content_types_migration = include_str!("../migrations/0006_content_types.sql");
        assert!(content_types_migration.contains("primary key (site_id, kind)"));
        assert!(content_types_migration.contains("content_types_site_created"));
        assert!(
            content_types_migration.contains("alter table content_types force row level security")
        );

        let slug_history_migration = include_str!("../migrations/0007_content_slug_history.sql");
        assert!(
            slug_history_migration.contains("primary key (site_id, content_id, language, slug)")
        );
        assert!(slug_history_migration.contains("content_slug_history_lookup"));
        assert!(
            slug_history_migration
                .contains("alter table content_slug_history force row level security")
        );

        let taxonomy_migration = include_str!("../migrations/0008_taxonomy.sql");
        assert!(taxonomy_migration.contains("primary key (site_id, id)"));
        assert!(taxonomy_migration.contains("taxonomy_terms_site_kind_language_slug"));
        assert!(
            taxonomy_migration.contains(
                "foreign key (site_id, parent_id) references taxonomy_terms(site_id, id)"
            )
        );
        assert!(taxonomy_migration.contains("primary key (site_id, content_id, term_id)"));
        assert!(
            taxonomy_migration
                .contains("alter table content_term_assignments force row level security")
        );

        let media_migration = include_str!("../migrations/0009_media.sql");
        assert!(media_migration.contains("primary key (site_id, id)"));
        assert!(media_migration.contains("unique (site_id, storage_key)"));
        assert!(media_migration.contains("media_files_site_kind_recent"));
        assert!(media_migration.contains("alter table media_files force row level security"));

        let media_visibility_migration = include_str!("../migrations/0030_media_visibility.sql");
        assert!(media_visibility_migration.contains("add column visibility text"));
        assert!(media_visibility_migration.contains("visibility in ('private', 'public')"));

        let cleanup_migration = include_str!("../migrations/0010_media_cleanup.sql");
        assert!(cleanup_migration.contains("primary key (site_id, file_id)"));
        assert!(cleanup_migration.contains("media_cleanup_tasks_pending"));
        assert!(
            cleanup_migration.contains("alter table media_cleanup_tasks force row level security")
        );

        let audit_immutable_migration = include_str!("../migrations/0011_audit_immutable.sql");
        assert!(audit_immutable_migration.contains("audit_events_append_only"));
        assert!(audit_immutable_migration.contains("revoke update, delete on audit_events"));

        let canonical_url_migration = include_str!("../migrations/0029_canonical_site_url.sql");
        assert!(canonical_url_migration.contains("add column canonical_url text"));
        assert!(canonical_url_migration.contains("site_settings_canonical_url_length"));

        let design_migration = include_str!("../migrations/0012_design.sql");
        assert!(design_migration.contains("primary key (site_id, id)"));
        assert!(design_migration.contains("design_changes_one_published"));
        assert!(
            design_migration.contains(
                "foreign key (site_id, change_id) references design_changes(site_id, id)"
            )
        );
        assert!(design_migration.contains("force row level security"));
        assert!(design_migration.contains("design_build_artifacts"));

        let forms_migration = include_str!("../migrations/0013_forms.sql");
        assert!(forms_migration.contains("primary key (site_id, id)"));
        assert!(forms_migration.contains("forms_site_slug_active"));
        assert!(forms_migration.contains("form_submissions_site_form_recent"));
        assert!(forms_migration.contains("force row level security"));

        let mail_migration = include_str!("../migrations/0014_mail.sql");
        assert!(mail_migration.contains("primary key (site_id, id)"));
        assert!(mail_migration.contains("mail_templates_site_key_language_active"));
        assert!(mail_migration.contains("mail_deliveries_site_queue"));
        assert!(mail_migration.contains(
            "foreign key (site_id, delivery_id) references mail_deliveries(site_id, id)"
        ));
        assert!(mail_migration.contains("mail_delivery_attempts"));
        assert!(mail_migration.contains("force row level security"));

        let shop_migration = include_str!("../migrations/0015_shop.sql");
        assert!(shop_migration.contains("primary key (site_id, id)"));
        assert!(shop_migration.contains("shop_products_site_slug_active"));
        assert!(shop_migration.contains("shop_orders_site_email_idempotency"));
        assert!(
            shop_migration
                .contains("foreign key (site_id, order_id) references shop_orders(site_id, id)")
        );
        assert!(shop_migration.contains("shop_stock_holds_site_expired"));
        assert!(shop_migration.contains("force row level security"));

        let courses_migration = include_str!("../migrations/0016_courses.sql");
        assert!(courses_migration.contains("primary key (site_id, id)"));
        assert!(courses_migration.contains("courses_site_slug_active"));
        assert!(courses_migration.contains("course_modules_site_position"));
        assert!(courses_migration.contains("course_lessons_site_position"));
        assert!(courses_migration.contains("course_student_sessions"));
        assert!(courses_migration.contains("course_enrollments"));
        assert!(courses_migration.contains("course_progress"));
        assert!(
            courses_migration.contains(
                "foreign key (site_id, media_file_id) references media_files(site_id, id)"
            )
        );
        assert!(courses_migration.contains("force row level security"));

        let jobs_migration = include_str!("../migrations/0017_jobs.sql");
        assert!(jobs_migration.contains("primary key (site_id, id)"));
        assert!(jobs_migration.contains("jobs_site_kind_idempotency"));
        assert!(jobs_migration.contains("claimed_until"));
        assert!(jobs_migration.contains("force row level security"));

        let automation_migration = include_str!("../migrations/0018_automation_flows.sql");
        assert!(automation_migration.contains("automation_flows"));
        assert!(automation_migration.contains("automation_flow_steps"));
        assert!(automation_migration.contains("automation_runs"));
        assert!(automation_migration.contains("automation_run_steps"));
        assert!(automation_migration.contains("automation_runs_site_flow_source"));
        assert!(automation_migration.contains("force row level security"));

        let grant_migration = include_str!("../migrations/0019_automation_grants.sql");
        assert!(grant_migration.contains("role_grants_capability_check"));
        assert!(grant_migration.contains("api_key_grants_capability_check"));
        assert!(grant_migration.contains("'automation'"));
        assert!(grant_migration.contains("'analytics'"));

        let portable_grant_migration = include_str!("../migrations/0022_portable_grant.sql");
        assert!(portable_grant_migration.contains("'portable'"));
        assert!(portable_grant_migration.contains("role_grants_capability_check"));

        let credentials_migration = include_str!("../migrations/0023_credentials.sql");
        assert!(credentials_migration.contains("create table site_credentials"));
        assert!(credentials_migration.contains("site_credentials_active_name"));
        assert!(credentials_migration.contains("force row level security"));
        assert!(credentials_migration.contains("'credentials'"));

        let write_fence_migration = include_str!("../migrations/0024_site_write_fences.sql");
        assert!(write_fence_migration.contains("create table site_write_fences"));
        assert!(write_fence_migration.contains("fence_token uuid not null"));
        let single_site_cleanup = include_str!("../migrations/0047_single_site_cleanup.sql");
        assert!(single_site_cleanup.contains("drop table if exists site_write_fences"));
        assert!(single_site_cleanup.contains("drop column if exists status"));
        let workflow_migration = include_str!("../migrations/0044_workflows.sql");
        assert!(workflow_migration.contains("unique (site_id, idempotency_key)"));
        assert!(workflow_migration.contains("foreign key (site_id, idempotency_key)"));
        let workflow_key_migration =
            include_str!("../migrations/0048_site_scoped_workflow_keys.sql");
        assert!(workflow_key_migration.contains("primary key (site_id, idempotency_key)"));

        let password_recovery_migration = include_str!("../migrations/0025_password_recovery.sql");
        assert!(password_recovery_migration.contains("create table password_reset_tokens"));
        assert!(password_recovery_migration.contains("foreign key (site_id, person_id)"));
        assert!(password_recovery_migration.contains("force row level security"));

        let email_verification_migration =
            include_str!("../migrations/0026_email_verification.sql");
        assert!(email_verification_migration.contains("email_verified_at"));
        assert!(email_verification_migration.contains("create table email_verification_tokens"));
        assert!(email_verification_migration.contains("create table auth_request_throttles"));
        assert!(email_verification_migration.contains("force row level security"));
        let protected_mail_migration =
            include_str!("../migrations/0027_protected_mail_deliveries.sql");
        assert!(protected_mail_migration.contains("body_protected boolean not null default false"));
        assert!(
            protected_mail_migration
                .contains("check ((not body_protected) or body = '[protected]') not valid")
        );
        assert!(!protected_mail_migration.contains("validate constraint"));
        assert!(protected_mail_migration.contains("mail_delivery_secrets"));
        assert!(protected_mail_migration.contains("octet_length(ciphertext)"));
        assert!(protected_mail_migration.contains("force row level security"));
        let protected_mail_validation_migration =
            include_str!("../migrations/0028_validate_protected_mail_deliveries.sql");
        assert!(
            protected_mail_validation_migration
                .contains("validate constraint mail_deliveries_body_protection_check")
        );

        let boards_migration = include_str!("../migrations/0020_boards.sql");
        assert!(boards_migration.contains("primary key (site_id, id)"));
        assert!(boards_migration.contains("board_lists_site_position"));
        assert!(boards_migration.contains("board_cards_site_position"));
        assert!(boards_migration.contains("board_activity_immutable"));
        assert!(boards_migration.contains("force row level security"));

        let analytics_migration = include_str!("../migrations/0021_analytics.sql");
        assert!(analytics_migration.contains("analytics_events"));
        assert!(analytics_migration.contains("analytics_daily"));
        assert!(analytics_migration.contains("analytics_events_site_recent"));
        assert!(analytics_migration.contains("force row level security"));
        let system_actor_migration = include_str!("../migrations/0031_system_audit_actor.sql");
        assert!(system_actor_migration.contains("'system'"));
        let job_claim_fencing_migration = include_str!("../migrations/0032_job_claim_fencing.sql");
        assert!(job_claim_fencing_migration.contains("add column claim_token uuid"));
        assert!(job_claim_fencing_migration.contains("jobs_running_has_lease"));
        let role_ownership_migration = include_str!("../migrations/0033_role_ownership.sql");
        assert!(role_ownership_migration.contains("add column system_role boolean"));
        assert!(role_ownership_migration.contains("roles_system_role_protected"));
        assert!(role_ownership_migration.contains("role_grants_system_role_protected"));
        let media_variants_migration = include_str!("../migrations/0034_media_variants.sql");
        assert!(media_variants_migration.contains("create table media_variants"));
        assert!(media_variants_migration.contains("unique (site_id, source_file_id, preset)"));
        assert!(media_variants_migration.contains("foreign key (site_id, source_file_id)"));
        assert!(media_variants_migration.contains("add column storage_keys text[]"));
        let mail_deliverability_migration =
            include_str!("../migrations/0035_mail_deliverability.sql");
        assert!(mail_deliverability_migration.contains("mail_unsubscribe_tokens"));
        assert!(mail_deliverability_migration.contains("mail_delivery_links"));
        assert!(mail_deliverability_migration.contains("force row level security"));
        let mail_provider_events_migration =
            include_str!("../migrations/0036_mail_provider_events.sql");
        assert!(mail_provider_events_migration.contains("mail_provider_events"));
        assert!(mail_provider_events_migration.contains("unique (site_id, provider, event_id)"));
        assert!(mail_provider_events_migration.contains("force row level security"));
        let mail_sender_policy_migration =
            include_str!("../migrations/0037_mail_sender_policy.sql");
        assert!(mail_sender_policy_migration.contains("mail_sender_address"));
        assert!(mail_sender_policy_migration.contains("mail_sender_name"));
        let analytics_retention_migration =
            include_str!("../migrations/0038_analytics_retention_policy.sql");
        assert!(analytics_retention_migration.contains("analytics_raw_retention_days"));
        assert!(analytics_retention_migration.contains("analytics_aggregate_retention_days"));
        let course_instructors_migration =
            include_str!("../migrations/0039_course_instructors.sql");
        assert!(course_instructors_migration.contains("course_instructors"));
        assert!(course_instructors_migration.contains("foreign key (site_id, course_id)"));
        assert!(course_instructors_migration.contains("force row level security"));
        let trash_retention_migration =
            include_str!("../migrations/0041_trash_retention_policy.sql");
        assert!(trash_retention_migration.contains("trash_retention_days"));
        assert!(trash_retention_migration.contains("between 1 and 3650"));
        let boards_flows_trash_migration =
            include_str!("../migrations/0042_boards_flows_trash.sql");
        assert!(
            boards_flows_trash_migration.contains("trash_archived boolean not null default false")
        );
        assert!(boards_flows_trash_migration.contains("deleted_at timestamptz"));
        assert!(
            boards_flows_trash_migration.contains("trash_enabled boolean not null default false")
        );
        assert!(boards_flows_trash_migration.contains("before update on board_activity"));
        assert!(
            boards_flows_trash_migration
                .contains("where archived_at is null and deleted_at is null")
        );
        let workflow_fencing_migration =
            include_str!("../migrations/0052_workflow_execution_fencing.sql");
        assert!(workflow_fencing_migration.contains("add column if not exists attempts integer"));
        assert!(workflow_fencing_migration.contains("claim_token uuid"));
        assert!(workflow_fencing_migration.contains("workflow_runs_claim_expiry"));
        let namespaced_permission_migration =
            include_str!("../migrations/0053_namespaced_permissions.sql");
        assert!(namespaced_permission_migration.contains("role_grants_permission_check"));
        assert!(namespaced_permission_migration.contains("api_key_grants_permission_check"));
        assert!(namespaced_permission_migration.contains("writing.content.entry.list"));
        let permission_scope_migration =
            include_str!("../migrations/0054_permission_resource_scope.sql");
        assert!(permission_scope_migration.contains("resource_type"));
        assert!(permission_scope_migration.contains("permission, resource_type"));
        let permission_namespace_migration =
            include_str!("../migrations/0055_normalize_namespaced_permissions.sql");
        assert!(permission_namespace_migration.contains("canonical storage key"));
        assert!(permission_namespace_migration.contains("governance."));
        let nullable_projection_migration =
            include_str!("../migrations/0056_nullable_legacy_permission_projection.sql");
        assert!(nullable_projection_migration.contains("drop not null"));
        assert!(nullable_projection_migration.contains("legacy_projection_check"));
        assert_eq!(CURRENT_SCHEMA_VERSION, 57);
    }

    #[test]
    fn identity_grant_constraints_cover_the_core_capability_registry() {
        let migration = include_str!("../migrations/0040_feedback.sql");
        for capability in Capability::ALL {
            assert!(migration.contains(&format!("'{}'", capability.as_str())));
        }
    }
}
