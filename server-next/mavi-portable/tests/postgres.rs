use std::env;

use chrono::Utc;
use mavi_core::{SiteContext, SiteId, ports::FileStore};
use mavi_design::{
    BuildEngine, DesignFileInput, DesignService, StartDesignChange, StaticBuildEngine,
};
use mavi_files::InMemoryFileStore;
use mavi_identity::{IdentityService, LoginInput, SetupInput};
use mavi_media::{FileVisibility, MediaService};
use mavi_portable::{
    ImportStrategy, PortableImportRequest, PortableRelocationRequest, PortableService,
};
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
    let files = InMemoryFileStore::default();
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

    IdentityService
        .initialize(
            &mut tx,
            &source_context,
            &SetupInput {
                site_name: "Source site".to_owned(),
                email: "owner@example.com".to_owned(),
                name: "Owner".to_owned(),
                password: "a-test-password-that-is-long-enough".to_owned(),
            },
        )
        .await
        .expect("source identity");

    let media_file = MediaService
        .upload(
            &mut tx,
            &source_context,
            &files,
            "hero.png",
            FileVisibility::Private,
            b"\x89PNG\r\n\x1a\n\x00\x00".to_vec(),
        )
        .await
        .expect("source media");
    let live_media_file = MediaService
        .upload(
            &mut tx,
            &source_context,
            &files,
            "live.png",
            FileVisibility::Public,
            b"\x89PNG\r\n\x1a\nlive".to_vec(),
        )
        .await
        .expect("live source media");

    let design = DesignService;
    let change = design
        .start_change(
            &mut tx,
            &source_context,
            &StartDesignChange {
                name: "Initial design".to_owned(),
            },
        )
        .await
        .expect("design change");
    design
        .write_file(
            &mut tx,
            &source_context,
            change.id,
            &DesignFileInput {
                path: "public/index.html".to_owned(),
                contents: "<h1>Source design</h1>".to_owned(),
            },
        )
        .await
        .expect("design source");
    let build_request = design
        .start_build(&mut tx, &source_context, change.id)
        .await
        .expect("design build start");
    let artifacts = StaticBuildEngine
        .build(
            &source_context,
            build_request.build.id,
            &build_request.source,
        )
        .await
        .expect("design build");
    let stored_artifacts = design
        .persist_artifacts(&source_context, &files, build_request.build.id, artifacts)
        .await
        .expect("design artifacts");
    design
        .finish_build_success(
            &mut tx,
            &source_context,
            build_request.build.id,
            &stored_artifacts,
        )
        .await
        .expect("design build ready");
    design
        .publish(&mut tx, &source_context, change.id)
        .await
        .expect("design publish");

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

    sqlx::query(
        "update content_entries set deleted_at = clock_timestamp()
          where site_id = $1 and id = $2",
    )
    .bind(source_site.into_uuid())
    .bind(content_id)
    .execute(tx.conn())
    .await
    .expect("trash source content");
    sqlx::query(
        "update taxonomy_terms set deleted_at = clock_timestamp()
          where site_id = $1 and id = $2",
    )
    .bind(source_site.into_uuid())
    .bind(term_id)
    .execute(tx.conn())
    .await
    .expect("trash source term");
    sqlx::query(
        "update media_files set deleted_at = clock_timestamp()
          where site_id = $1 and id = $2",
    )
    .bind(source_site.into_uuid())
    .bind(media_file.id.into_uuid())
    .execute(tx.conn())
    .await
    .expect("trash source media");

    let relocation_bundle = portable
        .export_for_relocation(&mut tx, &source_context, &files)
        .await
        .expect("identity relocation export");
    assert_eq!(relocation_bundle.identity.people.len(), 1);
    assert_eq!(relocation_bundle.identity.roles.len(), 1);
    assert_eq!(relocation_bundle.credentials.len(), 1);
    assert_eq!(relocation_bundle.media.files.len(), 1);
    assert_eq!(relocation_bundle.media.files[0].id, live_media_file.id);
    assert_eq!(relocation_bundle.design.changes.len(), 1);
    assert_eq!(relocation_bundle.design.files.len(), 1);
    assert_eq!(relocation_bundle.design.builds.len(), 1);
    assert_eq!(relocation_bundle.design.artifacts.len(), 1);
    assert!(!relocation_bundle.audit.events.is_empty());
    assert_eq!(relocation_bundle.trash.content.len(), 1);
    assert!(!relocation_bundle.trash.revisions.is_empty());
    assert_eq!(relocation_bundle.trash.terms.len(), 1);
    assert_eq!(relocation_bundle.trash.assignments.len(), 1);
    assert_eq!(relocation_bundle.trash.files.len(), 1);
    let source_audit = relocation_bundle.audit.events.clone();
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
    let mut relocation_bundle = relocation_bundle;
    relocation_bundle.bundle.manifest.source_site_id = relocation_site;
    relocation_bundle.audit.source_site_id = relocation_site;
    relocation_bundle.trash.source_site_id = relocation_site;
    relocation_bundle.forms.source_site_id = relocation_site;
    relocation_bundle.mail.source_site_id = relocation_site;
    relocation_bundle.shop.source_site_id = relocation_site;
    relocation_bundle.courses.source_site_id = relocation_site;
    relocation_bundle.jobs.source_site_id = relocation_site;
    relocation_bundle.flows.source_site_id = relocation_site;
    relocation_bundle.boards.source_site_id = relocation_site;
    relocation_bundle.analytics.source_site_id = relocation_site;
    let mut relocation_tx = database
        .begin(&relocation_context)
        .await
        .expect("relocation scope");
    portable
        .relocate(
            &mut relocation_tx,
            &relocation_context,
            &PortableRelocationRequest {
                bundle: relocation_bundle,
                strategy: ImportStrategy::Upsert,
            },
            &files,
        )
        .await
        .expect("relocation into a fresh site");
    let relocated = portable
        .export_for_relocation(&mut relocation_tx, &relocation_context, &files)
        .await
        .expect("relocated export");
    assert_eq!(relocated.bundle.site.name, "Source site");
    assert_eq!(relocated.identity.people.len(), 1);
    assert_eq!(relocated.identity.roles.len(), 1);
    assert_eq!(relocated.credentials.len(), 1);
    assert_eq!(relocated.media.files.len(), 1);
    assert_eq!(
        files
            .get(&relocation_context, &relocated.media.files[0].storage_key)
            .await
            .expect("relocated media"),
        b"\x89PNG\r\n\x1a\nlive"
    );
    assert_eq!(relocated.media.files[0].id, live_media_file.id);
    assert_eq!(relocated.trash.content.len(), 1);
    assert_eq!(relocated.trash.terms.len(), 1);
    assert_eq!(relocated.trash.assignments.len(), 1);
    assert_eq!(relocated.trash.files.len(), 1);
    assert_eq!(
        files
            .get(&relocation_context, &relocated.trash.files[0].storage_key)
            .await
            .expect("relocated trashed media"),
        b"\x89PNG\r\n\x1a\n\x00\x00"
    );
    assert_eq!(relocated.design.changes.len(), 1);
    assert_eq!(
        relocated.design.changes[0].state,
        mavi_design::DesignState::Published
    );
    assert_eq!(relocated.design.artifacts.len(), 1);
    assert!(!relocated.audit.events.is_empty());
    for event in source_audit {
        assert!(relocated.audit.events.contains(&event));
    }
    assert_eq!(
        files
            .get(
                &relocation_context,
                &relocated.design.artifacts[0].storage_key
            )
            .await
            .expect("relocated design artifact"),
        b"<h1>Source design</h1>"
    );
    IdentityService
        .create_session(
            &mut relocation_tx,
            &relocation_context,
            &LoginInput {
                email: "owner@example.com".to_owned(),
                password: "a-test-password-that-is-long-enough".to_owned(),
            },
            Utc::now(),
        )
        .await
        .expect("relocated owner can log in");
    relocation_tx.commit().await.expect("relocation commit");
}
