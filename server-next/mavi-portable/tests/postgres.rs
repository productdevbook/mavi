use std::env;

use chrono::Utc;
use mavi_core::{SiteContext, SiteId};
use mavi_portable::{ImportStrategy, PortableImportRequest, PortableService};
use mavi_storage::Database;
use serde_json::json;
use uuid::Uuid;

fn database_url() -> Option<String> {
    env::var("TEST_DATABASE_URL").ok()
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn portable_bundles_export_cross_site_import_and_reject_conflicts() {
    let url = database_url().expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 4).await.expect("database");
    database.migrate().await.expect("migrations");
    let source_site = SiteId::new();
    let target_site = SiteId::new();
    database
        .ensure_site(source_site)
        .await
        .expect("source site");
    database
        .ensure_site(target_site)
        .await
        .expect("target site");

    let content_id = Uuid::now_v7();
    let term_id = Uuid::now_v7();
    let now = Utc::now();
    let source_context = SiteContext::public(source_site);
    let mut tx = database.begin(&source_context).await.expect("source scope");
    sqlx::query("insert into site_settings (site_id, name, timezone) values ($1, $2, $3)")
        .bind(source_site.into_uuid())
        .bind("Source site")
        .bind("Europe/Berlin")
        .execute(tx.conn())
        .await
        .expect("source settings");
    sqlx::query(
        "insert into site_languages (site_id, tag, name, is_default) values ($1, $2, $3, true)",
    )
    .bind(source_site.into_uuid())
    .bind("en")
    .bind("English")
    .execute(tx.conn())
    .await
    .expect("source language");
    sqlx::query("insert into content_types (site_id, kind, name, fields) values ($1, $2, $3, $4)")
        .bind(source_site.into_uuid())
        .bind("post")
        .bind("Post")
        .bind(json!([]))
        .execute(tx.conn())
        .await
        .expect("source content type");
    sqlx::query("insert into taxonomy_terms (site_id, id, kind, language, slug, name) values ($1, $2, 'tag', 'en', 'intro', 'Intro')")
        .bind(source_site.into_uuid())
        .bind(term_id)
        .execute(tx.conn())
        .await
        .expect("source term");
    sqlx::query(
        "insert into content_entries
            (site_id, id, kind, language, slug, title, body, fields, status, revision)
         values ($1, $2, 'post', 'en', 'hello-world', 'Hello', 'Body', $3, 'draft', 1)",
    )
    .bind(source_site.into_uuid())
    .bind(content_id)
    .bind(json!({"featured": false}))
    .execute(tx.conn())
    .await
    .expect("source content");
    sqlx::query(
        "insert into content_revisions
            (site_id, content_id, revision, kind, language, slug, title, body, fields, status)
         values ($1, $2, 1, 'post', 'en', 'hello-world', 'Hello', 'Body', $3, 'draft')",
    )
    .bind(source_site.into_uuid())
    .bind(content_id)
    .bind(json!({"featured": false}))
    .execute(tx.conn())
    .await
    .expect("source revision");
    sqlx::query("insert into content_slug_history (site_id, content_id, language, slug, created_at) values ($1, $2, 'en', 'old-hello', $3)")
        .bind(source_site.into_uuid())
        .bind(content_id)
        .bind(now)
        .execute(tx.conn())
        .await
        .expect("source slug history");
    sqlx::query("insert into content_term_assignments (site_id, content_id, term_id, assigned_at) values ($1, $2, $3, $4)")
        .bind(source_site.into_uuid())
        .bind(content_id)
        .bind(term_id)
        .bind(now)
        .execute(tx.conn())
        .await
        .expect("source assignment");

    let portable = PortableService;
    let bundle = portable
        .export(&mut tx, &source_context)
        .await
        .expect("export");
    assert_eq!(bundle.manifest.source_site_id, source_site);
    assert_eq!(bundle.content.len(), 1);
    assert_eq!(bundle.revisions.len(), 1);
    assert_eq!(bundle.slug_history.len(), 1);
    assert_eq!(bundle.assignments.len(), 1);
    tx.commit().await.expect("source commit");

    let target_context = SiteContext::public(target_site);
    let mut tx = database.begin(&target_context).await.expect("target scope");
    sqlx::query("insert into site_settings (site_id, name) values ($1, 'Target site')")
        .bind(target_site.into_uuid())
        .execute(tx.conn())
        .await
        .expect("target settings");
    sqlx::query("insert into site_languages (site_id, tag, name, is_default) values ($1, 'en', 'English', true)")
        .bind(target_site.into_uuid())
        .execute(tx.conn())
        .await
        .expect("target language");

    let receipt = portable
        .import(
            &mut tx,
            &target_context,
            &PortableImportRequest {
                bundle: bundle.clone(),
                strategy: ImportStrategy::Upsert,
            },
        )
        .await
        .expect("import");
    assert_eq!(receipt.content, 1);
    assert_eq!(receipt.terms, 1);
    assert_eq!(receipt.strategy, ImportStrategy::Upsert);

    let imported = portable
        .export(&mut tx, &target_context)
        .await
        .expect("target export");
    assert_eq!(imported.site.name, "Source site");
    assert_eq!(imported.site.timezone, "Europe/Berlin");
    assert_eq!(imported.content[0].id, content_id);
    assert_eq!(imported.terms[0].id, term_id);
    assert_eq!(imported.assignments.len(), 1);

    let validation = portable
        .import(
            &mut tx,
            &target_context,
            &PortableImportRequest {
                bundle: bundle.clone(),
                strategy: ImportStrategy::ValidateOnly,
            },
        )
        .await
        .expect("validate-only import");
    assert_eq!(validation.content, 1);
    assert_eq!(validation.strategy, ImportStrategy::ValidateOnly);
    tx.commit().await.expect("target commit");

    let mut conflict_tx = database
        .begin(&target_context)
        .await
        .expect("conflict scope");
    let conflict = portable
        .import(
            &mut conflict_tx,
            &target_context,
            &PortableImportRequest {
                bundle: bundle.clone(),
                strategy: ImportStrategy::CreateOnly,
            },
        )
        .await
        .expect_err("create-only import must reject existing rows");
    assert!(matches!(conflict, mavi_core::MaviError::Conflict { .. }));

    let relocation_site = SiteId::new();
    database
        .ensure_site(relocation_site)
        .await
        .expect("relocation site");
    let relocation_context = SiteContext::public(relocation_site);
    let mut relocation_bundle = bundle;
    relocation_bundle.manifest.source_site_id = relocation_site;
    let mut relocation_tx = database
        .begin(&relocation_context)
        .await
        .expect("relocation scope");
    portable
        .relocate(
            &mut relocation_tx,
            &relocation_context,
            &PortableImportRequest {
                bundle: relocation_bundle,
                strategy: ImportStrategy::Upsert,
            },
        )
        .await
        .expect("relocation into a fresh site");
    let relocated = portable
        .export(&mut relocation_tx, &relocation_context)
        .await
        .expect("relocated export");
    assert_eq!(relocated.site.name, "Source site");
    relocation_tx.commit().await.expect("relocation commit");
}
