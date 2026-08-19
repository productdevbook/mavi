//! `PostgreSQL` access that cannot forget the current site scope.
//!
//! Domain code receives a [`SiteTx`] rather than a pool. The pool is private,
//! and the transaction sets the `PostgreSQL` scope with `SET LOCAL`, so a
//! connection returned to the pool cannot carry one request's site into the
//! next request.

use mavi_core::{MaviError, Result, SiteContext, SiteId};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// The highest migration applied by this workspace.
///
/// It is part of the runtime compatibility contract exposed to the operator.
/// Keep it next to the migration runner so a release cannot advertise a
/// storage version independently from the migrations it ships.
pub const CURRENT_SCHEMA_VERSION: u32 = 38;

/// The lifecycle state stored in the shared shard catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SiteStatus {
    Provisioning,
    Active,
    Suspended,
    Removed,
}

impl SiteStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Provisioning => "provisioning",
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Removed => "removed",
        }
    }
}

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
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|_| MaviError::Internal)
    }

    /// Checks the database connection used by runtime readiness probes.
    ///
    /// This intentionally does not open a site-scoped transaction: readiness
    /// is a process/shard concern, not a request for one site's data.
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("select 1")
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|_| MaviError::Internal)
    }

    /// Creates a site catalog row without exposing an unscoped transaction to domains.
    pub async fn ensure_site(&self, site_id: SiteId) -> Result<()> {
        self.reconcile_sites([(site_id, SiteStatus::Active)]).await
    }

    /// Applies a control-plane lifecycle snapshot as one catalog transaction.
    ///
    /// The caller owns host routing; this method only makes the shard's
    /// durable status agree with the authoritative control snapshot. Existing
    /// rows are updated instead of being deleted so removed sites remain
    /// valid parents for retained audit and financial records.
    pub async fn reconcile_sites(
        &self,
        sites: impl IntoIterator<Item = (SiteId, SiteStatus)>,
    ) -> Result<()> {
        let mut transaction = self.pool.begin().await.map_err(|_| MaviError::Internal)?;

        for (site_id, status) in sites {
            sqlx::query(
                "insert into site_catalog (site_id, status)
                 values ($1, $2)
                 on conflict (site_id) do update set status = excluded.status",
            )
            .bind(site_id.into_uuid())
            .bind(status.as_str())
            .execute(&mut *transaction)
            .await
            .map_err(|_| MaviError::Internal)?;
        }

        transaction
            .commit()
            .await
            .map_err(|_| MaviError::Internal)?;

        Ok(())
    }

    /// Acquires a site write fence for a relocation operation.
    ///
    /// A fence is token-owned. Repeating the same acquisition is idempotent;
    /// a different token is refused so an old worker cannot take over or
    /// replace a newer cutover operation.
    pub async fn acquire_write_fence(
        &self,
        site_id: SiteId,
        fence_token: Uuid,
        reason: &str,
    ) -> Result<()> {
        if !(1..=120).contains(&reason.len()) || reason.chars().any(char::is_control) {
            return Err(MaviError::validation("site_write_fence_reason_invalid"));
        }

        let mut transaction = self.pool.begin().await.map_err(|_| MaviError::Internal)?;
        let existing: Option<Uuid> = sqlx::query_scalar(
            "select fence_token from site_write_fences where site_id = $1 for update",
        )
        .bind(site_id.into_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| MaviError::Internal)?;

        match existing {
            Some(existing) if existing != fence_token => {
                return Err(MaviError::conflict("site_write_fence_owned"));
            }
            Some(_) => {}
            None => {
                sqlx::query(
                    "insert into site_write_fences (site_id, fence_token, reason)
                     values ($1, $2, $3)",
                )
                .bind(site_id.into_uuid())
                .bind(fence_token)
                .bind(reason)
                .execute(&mut *transaction)
                .await
                .map_err(|_| MaviError::Internal)?;
            }
        }

        transaction.commit().await.map_err(|_| MaviError::Internal)
    }

    /// Releases only the fence owned by `fence_token`. Releasing an already
    /// gone fence is idempotent; a different active fence is left untouched.
    pub async fn release_write_fence(&self, site_id: SiteId, fence_token: Uuid) -> Result<()> {
        sqlx::query(
            "delete from site_write_fences
              where site_id = $1 and fence_token = $2",
        )
        .bind(site_id.into_uuid())
        .bind(fence_token)
        .execute(&self.pool)
        .await
        .map_err(|_| MaviError::Internal)?;
        Ok(())
    }

    /// Reads the durable fence without opening a site-scoped transaction.
    pub async fn is_write_fenced(&self, site_id: SiteId) -> Result<bool> {
        sqlx::query_scalar("select exists(select 1 from site_write_fences where site_id = $1)")
            .bind(site_id.into_uuid())
            .fetch_one(&self.pool)
            .await
            .map_err(|_| MaviError::Internal)
    }

    pub async fn begin(&self, context: &SiteContext) -> Result<SiteTx> {
        let mut transaction = self.pool.begin().await.map_err(|_| MaviError::Internal)?;
        sqlx::query("select set_config('app.site_id', $1, true)")
            .bind(context.site_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(|_| MaviError::Internal)?;

        Ok(SiteTx { transaction })
    }
}

#[derive(Debug)]
pub struct SiteTx {
    transaction: Transaction<'static, Postgres>,
}

impl SiteTx {
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
    use crate::{CURRENT_SCHEMA_VERSION, SiteStatus};

    #[test]
    fn site_statuses_match_the_catalog_contract() {
        assert_eq!(SiteStatus::Provisioning.as_str(), "provisioning");
        assert_eq!(SiteStatus::Active.as_str(), "active");
        assert_eq!(SiteStatus::Suspended.as_str(), "suspended");
        assert_eq!(SiteStatus::Removed.as_str(), "removed");
    }

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
        assert_eq!(CURRENT_SCHEMA_VERSION, 38);
    }
}
