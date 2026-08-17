//! `PostgreSQL` access that cannot forget the current site scope.
//!
//! Domain code receives a [`SiteTx`] rather than a pool. The pool is private,
//! and the transaction sets the `PostgreSQL` scope with `SET LOCAL`, so a
//! connection returned to the pool cannot carry one request's site into the
//! next request.

use mavi_core::{MaviError, Result, SiteContext, SiteId};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Postgres, Transaction};

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

    /// Creates a site catalog row without exposing an unscoped transaction to domains.
    pub async fn ensure_site(&self, site_id: SiteId) -> Result<()> {
        sqlx::query(
            "insert into site_catalog (site_id) values ($1) on conflict (site_id) do nothing",
        )
        .bind(site_id.into_uuid())
        .execute(&self.pool)
        .await
        .map_err(|_| MaviError::Internal)?;

        Ok(())
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
    #[test]
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

        let cleanup_migration = include_str!("../migrations/0010_media_cleanup.sql");
        assert!(cleanup_migration.contains("primary key (site_id, file_id)"));
        assert!(cleanup_migration.contains("media_cleanup_tasks_pending"));
        assert!(
            cleanup_migration.contains("alter table media_cleanup_tasks force row level security")
        );

        let audit_immutable_migration = include_str!("../migrations/0011_audit_immutable.sql");
        assert!(audit_immutable_migration.contains("audit_events_append_only"));
        assert!(audit_immutable_migration.contains("revoke update, delete on audit_events"));

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
    }
}
