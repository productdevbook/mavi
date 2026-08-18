use std::env;

use chrono::Utc;
use mavi_core::{SiteContext, SiteId};
use mavi_files::InMemoryFileStore;
use mavi_portable::{ImportStrategy, PortableRelocationRequest, PortableService};
use mavi_storage::Database;
use serde_json::json;
use uuid::Uuid;

fn database_url() -> Option<String> {
    env::var("TEST_DATABASE_URL").ok()
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn forms_and_mail_relocation_preserves_data_and_resets_delivery_leases() {
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

    let form_id = Uuid::now_v7();
    let deleted_form_id = Uuid::now_v7();
    let submission_id = Uuid::now_v7();
    let template_id = Uuid::now_v7();
    let list_id = Uuid::now_v7();
    let reader_id = Uuid::now_v7();
    let delivery_id = Uuid::now_v7();
    let attempt_id = Uuid::now_v7();
    let now = Utc::now();
    let context = SiteContext::public(source_site);
    let mut tx = database.begin(&context).await.expect("source scope");

    sqlx::query(
        "insert into site_settings (site_id, name, timezone) values ($1, 'Forms and mail', 'UTC')",
    )
    .bind(source_site.into_uuid())
    .execute(tx.conn())
    .await
    .expect("source settings");
    sqlx::query(
        "insert into site_languages (site_id, tag, name, is_default)
         values ($1, 'en', 'English', true)",
    )
    .bind(source_site.into_uuid())
    .execute(tx.conn())
    .await
    .expect("source language");
    sqlx::query(
        "insert into forms (site_id, id, slug, name, fields, kept_days)
         values ($1, $2, 'contact', 'Contact', $3, 90)",
    )
    .bind(source_site.into_uuid())
    .bind(form_id)
    .bind(json!([{
        "key": "email",
        "label": "Email",
        "required": true,
        "kind": "email",
        "options": []
    }]))
    .execute(tx.conn())
    .await
    .expect("source form");
    sqlx::query(
        "insert into forms (site_id, id, slug, name, fields, open, kept_days, deleted_at)
         values ($1, $2, 'old-contact', 'Old contact', $3, false, 365, $4)",
    )
    .bind(source_site.into_uuid())
    .bind(deleted_form_id)
    .bind(json!([]))
    .bind(now)
    .execute(tx.conn())
    .await
    .expect("deleted source form");
    sqlx::query(
        "insert into form_submissions (site_id, id, form_id, answers, seen_at, created_at, deleted_at)
         values ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(source_site.into_uuid())
    .bind(submission_id)
    .bind(form_id)
    .bind(json!({"email": "person@example.test"}))
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(tx.conn())
    .await
    .expect("source submission");

    sqlx::query(
        "insert into mail_templates (site_id, id, template_key, language, subject, body)
         values ($1, $2, 'welcome', 'en', 'Welcome', 'Hello')",
    )
    .bind(source_site.into_uuid())
    .bind(template_id)
    .execute(tx.conn())
    .await
    .expect("source template");
    sqlx::query(
        "insert into mail_lists (site_id, id, slug, name)
         values ($1, $2, 'subscribers', 'Subscribers')",
    )
    .bind(source_site.into_uuid())
    .bind(list_id)
    .execute(tx.conn())
    .await
    .expect("source list");
    sqlx::query(
        "insert into mail_readers
            (site_id, id, email, name, unsubscribe_token_hash)
         values ($1, $2, 'person@example.test', 'Person', $3)",
    )
    .bind(source_site.into_uuid())
    .bind(reader_id)
    .bind(vec![7_u8; 32])
    .execute(tx.conn())
    .await
    .expect("source reader");
    sqlx::query(
        "insert into mail_list_members (site_id, list_id, reader_id, created_at)
         values ($1, $2, $3, $4)",
    )
    .bind(source_site.into_uuid())
    .bind(list_id)
    .bind(reader_id)
    .bind(now)
    .execute(tx.conn())
    .await
    .expect("source membership");
    sqlx::query(
        "insert into mail_deliveries
            (site_id, id, template_id, list_id, recipient, subject, body, content_type,
             purpose, status, attempts, available_at, lease_owner, lease_until,
             provider, provider_reference, last_error, idempotency_key, created_at, updated_at)
         values ($1, $2, $3, $4, 'person@example.test', 'Welcome', 'Hello', 'plain',
                 'campaign', 'sending', 1, $5, 'worker-1', $6, 'smtp', 'ref-1', null,
                 'campaign-1', $7, $7)",
    )
    .bind(source_site.into_uuid())
    .bind(delivery_id)
    .bind(template_id)
    .bind(list_id)
    .bind(now)
    .bind(now + chrono::Duration::minutes(5))
    .bind(now)
    .execute(tx.conn())
    .await
    .expect("source delivery");
    sqlx::query(
        "insert into mail_delivery_attempts
            (site_id, id, delivery_id, attempt_number, status, started_at)
         values ($1, $2, $3, 1, 'sending', $4)",
    )
    .bind(source_site.into_uuid())
    .bind(attempt_id)
    .bind(delivery_id)
    .bind(now)
    .execute(tx.conn())
    .await
    .expect("source attempt");

    let files = InMemoryFileStore::default();
    let portable = PortableService;
    let mut relocation = portable
        .export_for_relocation(&mut tx, &context, &files)
        .await
        .expect("relocation export");
    assert_eq!(relocation.forms.forms.len(), 2);
    assert_eq!(relocation.forms.submissions.len(), 1);
    assert_eq!(relocation.mail.templates.len(), 1);
    assert_eq!(relocation.mail.memberships.len(), 1);
    assert_eq!(relocation.mail.deliveries[0].status, "retry");
    assert_eq!(relocation.mail.attempts[0].status, "retry");
    assert!(relocation.mail.attempts[0].finished_at.is_some());
    tx.commit().await.expect("source commit");

    relocation.bundle.manifest.source_site_id = target_site;
    relocation.audit.source_site_id = target_site;
    relocation.trash.source_site_id = target_site;
    relocation.forms.source_site_id = target_site;
    relocation.mail.source_site_id = target_site;

    let target_context = SiteContext::public(target_site);
    let mut target_tx = database.begin(&target_context).await.expect("target scope");
    portable
        .relocate(
            &mut target_tx,
            &target_context,
            &PortableRelocationRequest {
                bundle: relocation.clone(),
                strategy: ImportStrategy::Upsert,
            },
            &files,
        )
        .await
        .expect("relocation import");

    let relocated = portable
        .export_for_relocation(&mut target_tx, &target_context, &files)
        .await
        .expect("target export");
    assert_eq!(relocated.forms.forms, relocation.forms.forms);
    assert_eq!(relocated.forms.submissions, relocation.forms.submissions);
    assert_eq!(relocated.mail.templates, relocation.mail.templates);
    assert_eq!(relocated.mail.lists, relocation.mail.lists);
    assert_eq!(relocated.mail.readers, relocation.mail.readers);
    assert_eq!(relocated.mail.memberships, relocation.mail.memberships);
    assert_eq!(relocated.mail.deliveries, relocation.mail.deliveries);
    assert_eq!(relocated.mail.attempts, relocation.mail.attempts);

    let lease: (Option<String>, Option<chrono::DateTime<Utc>>) = sqlx::query_as(
        "select lease_owner, lease_until from mail_deliveries where site_id = $1 and id = $2",
    )
    .bind(target_site.into_uuid())
    .bind(delivery_id)
    .fetch_one(target_tx.conn())
    .await
    .expect("target lease");
    assert_eq!(lease, (None, None));
    target_tx.commit().await.expect("target commit");
}
