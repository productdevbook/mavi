use std::{env, sync::Arc};

use chrono::{Duration, Utc};
use mavi_analytics::ANALYTICS_RETENTION_JOB;
use mavi_content::{
    ContentService, CreateContent, Publication, PublicationInput, SCHEDULED_PUBLISH_JOB,
    ScheduledPublishJob,
};
use mavi_core::{AnalyticsEventId, FormSubmissionId, SiteContext, SiteId, ports::FileStore};
use mavi_files::InMemoryFileStore;
use mavi_forms::{CreateForm, FORM_RETENTION_JOB, FormService};
use mavi_jobs::JobsService;
use mavi_media::{
    FileVariantListFilter, FileVisibility, MEDIA_CLEANUP_JOB, MEDIA_ORPHAN_CLEANUP_JOB,
    MEDIA_VARIANT_JOB, MediaService, VariantPreset,
};
use mavi_storage::Database;
use mavi_trash::{MAX_TRASH_RETENTION_BATCH, TRASH_RETENTION_JOB, TrashKind, TrashService};
use mavi_worker::{WorkerConfig, WorkerSupervisor};
use serde_json::to_value;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn scheduled_worker_publishes_skips_stale_and_defers_early_jobs() {
    let database_url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&database_url, 4)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");
    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");

    let content = ContentService;
    let jobs = JobsService::new([SCHEDULED_PUBLISH_JOB]);
    let request_context = SiteContext::public(site_id);
    let mut transaction = database.begin(&request_context).await.expect("scope");
    let created = content
        .create(
            &mut transaction,
            &request_context,
            &CreateContent {
                kind: "post".to_owned(),
                language: "en".to_owned(),
                slug: "worker-published".to_owned(),
                title: "Worker published".to_owned(),
                excerpt: None,
                body: "published by worker".to_owned(),
                fields: serde_json::json!({}),
                publication: PublicationInput::default(),
            },
            Utc::now(),
        )
        .await
        .expect("content");
    let requested_at = Utc::now() + Duration::milliseconds(100);
    let scheduled = content
        .schedule(
            &mut transaction,
            &request_context,
            created.id,
            requested_at,
            Utc::now(),
        )
        .await
        .expect("schedule");
    let Publication::Scheduled { at: scheduled_at } = scheduled.publication else {
        panic!("content should be scheduled")
    };
    let job_id = jobs
        .enqueue(
            &mut transaction,
            &request_context,
            SCHEDULED_PUBLISH_JOB.name,
            &to_value(ScheduledPublishJob {
                content_id: created.id,
                scheduled_at,
            })
            .expect("payload"),
            Some(scheduled_at),
            Some(&format!(
                "worker-test:{created_id}",
                created_id = created.id
            )),
        )
        .await
        .expect("job");
    transaction.commit().await.expect("commit");

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let supervisor = WorkerSupervisor::new(
        database.clone(),
        [site_id],
        WorkerConfig::new(
            "test-content-worker",
            30,
            std::time::Duration::from_millis(10),
        )
        .expect("worker config"),
        Arc::new(InMemoryFileStore::default()),
    );
    assert!(supervisor.run_once(site_id).await.expect("publish run"));
    let metrics = supervisor.metrics().snapshot();
    assert_eq!(metrics.polls, 1);
    assert_eq!(metrics.claims, 1);
    assert_eq!(metrics.completed, 1);
    assert_eq!(metrics.failed, 0);
    assert_eq!(metrics.deferred, 0);
    assert_eq!(metrics.lost_leases, 0);
    assert_eq!(metrics.errors, 0);

    let mut transaction = database
        .begin(&request_context)
        .await
        .expect("content scope");
    let published = content
        .get(&mut transaction, &request_context, created.id)
        .await
        .expect("published content");
    assert!(matches!(
        published.publication,
        Publication::Published { .. }
    ));
    let job = jobs.get(&mut transaction, job_id).await.expect("job state");
    assert_eq!(job.state.as_str(), "done");
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&request_context).await.expect("audit scope");
    let (actor_kind, actor_id): (String, Option<String>) = sqlx::query_as(
        "select actor_kind, actor_id from audit_events
          where site_id = $1 and action = 'content.published'
          order by created_at desc limit 1",
    )
    .bind(site_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("publish audit");
    assert_eq!(actor_kind, "system");
    assert_eq!(actor_id.as_deref(), Some("test-content-worker"));
    transaction.commit().await.expect("commit");

    let stale_content = {
        let mut transaction = database.begin(&request_context).await.expect("scope");
        let created = content
            .create(
                &mut transaction,
                &request_context,
                &CreateContent {
                    kind: "post".to_owned(),
                    language: "en".to_owned(),
                    slug: "worker-stale".to_owned(),
                    title: "Worker stale".to_owned(),
                    excerpt: None,
                    body: "stale schedule".to_owned(),
                    fields: serde_json::json!({}),
                    publication: PublicationInput::default(),
                },
                Utc::now(),
            )
            .await
            .expect("content");
        let current_schedule = Utc::now() + Duration::hours(1);
        content
            .schedule(
                &mut transaction,
                &request_context,
                created.id,
                current_schedule,
                Utc::now(),
            )
            .await
            .expect("schedule");
        jobs.enqueue(
            &mut transaction,
            &request_context,
            SCHEDULED_PUBLISH_JOB.name,
            &to_value(ScheduledPublishJob {
                content_id: created.id,
                scheduled_at: Utc::now() - Duration::seconds(1),
            })
            .expect("payload"),
            Some(Utc::now() - Duration::seconds(1)),
            Some(&format!(
                "worker-stale:{created_id}",
                created_id = created.id
            )),
        )
        .await
        .expect("stale job");
        transaction.commit().await.expect("commit");
        created.id
    };
    assert!(supervisor.run_once(site_id).await.expect("stale run"));

    let mut transaction = database.begin(&request_context).await.expect("scope");
    let stale = content
        .get(&mut transaction, &request_context, stale_content)
        .await
        .expect("stale content");
    assert!(matches!(stale.publication, Publication::Scheduled { .. }));
    let (skip_count,): (i64,) = sqlx::query_as(
        "select count(*) from audit_events
          where site_id = $1 and action = 'content.publish_scheduled_skipped'",
    )
    .bind(site_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("skip audit");
    assert_eq!(skip_count, 1);
    transaction.commit().await.expect("commit");

    let early_content = {
        let mut transaction = database.begin(&request_context).await.expect("scope");
        let created = content
            .create(
                &mut transaction,
                &request_context,
                &CreateContent {
                    kind: "post".to_owned(),
                    language: "en".to_owned(),
                    slug: "worker-early".to_owned(),
                    title: "Worker early".to_owned(),
                    excerpt: None,
                    body: "early retry".to_owned(),
                    fields: serde_json::json!({}),
                    publication: PublicationInput::default(),
                },
                Utc::now(),
            )
            .await
            .expect("content");
        let scheduled_at = Utc::now() + Duration::hours(1);
        jobs.enqueue(
            &mut transaction,
            &request_context,
            SCHEDULED_PUBLISH_JOB.name,
            &to_value(ScheduledPublishJob {
                content_id: created.id,
                scheduled_at,
            })
            .expect("payload"),
            Some(Utc::now() - Duration::seconds(1)),
            Some(&format!(
                "worker-early:{created_id}",
                created_id = created.id
            )),
        )
        .await
        .expect("early job");
        transaction.commit().await.expect("commit");
        (created.id, scheduled_at)
    };
    assert!(supervisor.run_once(site_id).await.expect("early run"));

    let mut transaction = database.begin(&request_context).await.expect("scope");
    let early_page = jobs
        .list(
            &mut transaction,
            &mavi_jobs::JobListFilter {
                kind: Some(SCHEDULED_PUBLISH_JOB.name.to_owned()),
                ..Default::default()
            },
        )
        .await
        .expect("jobs");
    let early_job = early_page
        .items
        .iter()
        .find(|job| job.payload["content_id"] == early_content.0.to_string())
        .expect("deferred job");
    assert_eq!(early_job.state.as_str(), "ready");
    assert_eq!(
        early_job.run_at.timestamp_micros(),
        early_content.1.timestamp_micros()
    );
    transaction.commit().await.expect("commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn form_retention_worker_prunes_expired_submissions_and_records_system_audit() {
    let database_url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&database_url, 4)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");
    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");

    let context = SiteContext::public(site_id);
    let forms = FormService;
    let form = {
        let mut transaction = database.begin(&context).await.expect("form scope");
        let form = forms
            .create(
                &mut transaction,
                &context,
                &CreateForm {
                    slug: "retention".to_owned(),
                    name: "Retention".to_owned(),
                    fields: Vec::new(),
                    kept_days: Some(1),
                },
            )
            .await
            .expect("form");
        transaction.commit().await.expect("form commit");
        form
    };
    let submission_id = FormSubmissionId::new();
    let mut transaction = database.begin(&context).await.expect("submission scope");
    sqlx::query(
        "insert into form_submissions (site_id, id, form_id, answers, created_at)
         values ($1, $2, $3, '{}'::jsonb, $4)",
    )
    .bind(site_id.into_uuid())
    .bind(submission_id.into_uuid())
    .bind(form.id.into_uuid())
    .bind(Utc::now() - Duration::days(2))
    .execute(transaction.conn())
    .await
    .expect("expired submission");
    transaction.commit().await.expect("submission commit");

    let store = Arc::new(InMemoryFileStore::default());
    let supervisor = WorkerSupervisor::new(
        database.clone(),
        [site_id],
        WorkerConfig::new(
            "test-form-retention-worker",
            30,
            std::time::Duration::from_millis(10),
        )
        .expect("worker config"),
        store,
    );
    assert!(supervisor.run_once(site_id).await.expect("orphan run"));
    assert!(supervisor.run_once(site_id).await.expect("retention run"));

    let jobs = JobsService::new([FORM_RETENTION_JOB]);
    let mut transaction = database.begin(&context).await.expect("check scope");
    let deleted: bool = sqlx::query_scalar(
        "select deleted_at is not null from form_submissions where site_id = $1 and id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(submission_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("deleted state");
    assert!(deleted);
    let retention_jobs = jobs
        .list(
            &mut transaction,
            &mavi_jobs::JobListFilter {
                kind: Some(FORM_RETENTION_JOB.name.to_owned()),
                ..Default::default()
            },
        )
        .await
        .expect("retention jobs");
    assert_eq!(retention_jobs.items.len(), 1);
    assert_eq!(retention_jobs.items[0].state.as_str(), "done");
    let (actor_kind, actor_id): (String, Option<String>) = sqlx::query_as(
        "select actor_kind, actor_id from audit_events
          where site_id = $1 and action = 'forms.submissions.retention_pruned'
          order by created_at desc limit 1",
    )
    .bind(site_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("retention audit");
    assert_eq!(actor_kind, "system");
    assert_eq!(actor_id.as_deref(), Some("test-form-retention-worker"));
    transaction.commit().await.expect("check commit");
}

async fn seed_expired_trash_content(database: &Database, site_id: SiteId) -> uuid::Uuid {
    let request_context = SiteContext::public(site_id);
    let content_service = ContentService;
    let mut transaction = database
        .begin(&request_context)
        .await
        .expect("content scope");
    sqlx::query("insert into site_settings (site_id, name) values ($1, 'Trash retention')")
        .bind(site_id.into_uuid())
        .execute(transaction.conn())
        .await
        .expect("site settings");
    sqlx::query("update site_settings set trash_retention_days = 1 where site_id = $1")
        .bind(site_id.into_uuid())
        .execute(transaction.conn())
        .await
        .expect("trash policy");
    let created = content_service
        .create(
            &mut transaction,
            &request_context,
            &CreateContent {
                kind: "post".to_owned(),
                language: "en".to_owned(),
                slug: "expired-trash-content".to_owned(),
                title: "Expired trash content".to_owned(),
                excerpt: None,
                body: "removed by retention".to_owned(),
                fields: serde_json::json!({}),
                publication: PublicationInput::default(),
            },
            Utc::now(),
        )
        .await
        .expect("content");
    content_service
        .trash(&mut transaction, &request_context, created.id)
        .await
        .expect("trash content");
    sqlx::query(
        "update content_entries
            set deleted_at = $3
          where site_id = $1 and id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(created.id.into_uuid())
    .bind(Utc::now() - Duration::days(2))
    .execute(transaction.conn())
    .await
    .expect("age trash content");
    transaction.commit().await.expect("seed commit");
    created.id.into_uuid()
}

async fn seed_expired_trash_content_batch(
    database: &Database,
    site_id: SiteId,
    count: usize,
) -> usize {
    let context = SiteContext::public(site_id);
    let mut transaction = database.begin(&context).await.expect("content scope");
    sqlx::query("insert into site_settings (site_id, name) values ($1, 'Trash retention batch')")
        .bind(site_id.into_uuid())
        .execute(transaction.conn())
        .await
        .expect("site settings");
    sqlx::query("update site_settings set trash_retention_days = 1 where site_id = $1")
        .bind(site_id.into_uuid())
        .execute(transaction.conn())
        .await
        .expect("trash policy");
    let deleted_at = Utc::now() - Duration::days(2);
    for index in 0..count {
        sqlx::query(
            "insert into content_entries
                (site_id, id, kind, language, slug, title, body, fields, status, deleted_at)
             values ($1, $2, 'post', 'en', $3, $4, 'expired', '{}'::jsonb, 'draft', $5)",
        )
        .bind(site_id.into_uuid())
        .bind(Uuid::now_v7())
        .bind(format!("expired-trash-batch-{index}"))
        .bind(format!("Expired trash batch {index}"))
        .bind(deleted_at)
        .execute(transaction.conn())
        .await
        .expect("expired content");
    }
    transaction.commit().await.expect("seed commit");
    count
}

async fn run_until_job_done(
    supervisor: &WorkerSupervisor,
    database: &Database,
    site_id: SiteId,
    context: &SiteContext,
    kind: &str,
) -> bool {
    for _ in 0..=8 {
        supervisor.run_once(site_id).await.expect("retention run");
        let mut check = database.begin(context).await.expect("check scope");
        let state: Option<String> = sqlx::query_scalar(
            "select state from jobs
               where site_id = $1 and kind = $2
               order by created_at desc limit 1",
        )
        .bind(context.site_id.into_uuid())
        .bind(kind)
        .fetch_optional(check.conn())
        .await
        .expect("retention job state");
        check.commit().await.expect("check commit");
        if state.as_deref() == Some("done") {
            return true;
        }
        tokio::task::yield_now().await;
    }
    false
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn trash_retention_worker_removes_expired_content_and_audits_system_work() {
    let database_url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&database_url, 4)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");
    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");

    let context = SiteContext::public(site_id);
    let content_id = seed_expired_trash_content(&database, site_id).await;

    let supervisor = WorkerSupervisor::new(
        database.clone(),
        [site_id],
        WorkerConfig::new(
            "test-trash-retention-worker",
            30,
            std::time::Duration::from_millis(10),
        )
        .expect("worker config"),
        Arc::new(InMemoryFileStore::default()),
    );

    assert!(
        run_until_job_done(
            &supervisor,
            &database,
            site_id,
            &context,
            TRASH_RETENTION_JOB.name,
        )
        .await,
        "trash retention job should complete"
    );

    let mut transaction = database.begin(&context).await.expect("assert scope");
    let content_exists: bool = sqlx::query_scalar(
        "select exists(select 1 from content_entries where site_id = $1 and id = $2)",
    )
    .bind(site_id.into_uuid())
    .bind(content_id)
    .fetch_one(transaction.conn())
    .await
    .expect("content state");
    assert!(!content_exists);
    let (actor_kind, actor_id): (String, Option<String>) = sqlx::query_as(
        "select actor_kind, actor_id from audit_events
          where site_id = $1 and action = 'trash.item.permanently_deleted'
          order by created_at desc limit 1",
    )
    .bind(site_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("retention audit");
    assert_eq!(actor_kind, "system");
    assert_eq!(actor_id.as_deref(), Some("test-trash-retention-worker"));
    transaction.commit().await.expect("assert commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn trash_retention_worker_removes_expired_forms_and_cascades_submissions() {
    let database_url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&database_url, 4)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");
    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");

    let context = SiteContext::public(site_id);
    let form_service = FormService;
    let form_id = {
        let mut transaction = database.begin(&context).await.expect("form scope");
        sqlx::query(
            "insert into site_settings (site_id, name) values ($1, 'Form trash retention')",
        )
        .bind(site_id.into_uuid())
        .execute(transaction.conn())
        .await
        .expect("site settings");
        sqlx::query("update site_settings set trash_retention_days = 1 where site_id = $1")
            .bind(site_id.into_uuid())
            .execute(transaction.conn())
            .await
            .expect("trash policy");
        let form = form_service
            .create(
                &mut transaction,
                &context,
                &CreateForm {
                    slug: "expired-form-trash".to_owned(),
                    name: "Expired form trash".to_owned(),
                    fields: Vec::new(),
                    kept_days: None,
                },
            )
            .await
            .expect("form");
        sqlx::query(
            "insert into form_submissions (site_id, id, form_id, answers)
             values ($1, $2, $3, '{}'::jsonb)",
        )
        .bind(site_id.into_uuid())
        .bind(Uuid::now_v7())
        .bind(form.id.into_uuid())
        .execute(transaction.conn())
        .await
        .expect("submission");
        form_service
            .delete(&mut transaction, &context, form.id)
            .await
            .expect("trash form");
        sqlx::query("update forms set deleted_at = $3 where site_id = $1 and id = $2")
            .bind(site_id.into_uuid())
            .bind(form.id.into_uuid())
            .bind(Utc::now() - Duration::days(2))
            .execute(transaction.conn())
            .await
            .expect("age form trash");
        transaction.commit().await.expect("seed commit");
        form.id.into_uuid()
    };

    let supervisor = WorkerSupervisor::new(
        database.clone(),
        [site_id],
        WorkerConfig::new(
            "test-form-trash-retention-worker",
            30,
            std::time::Duration::from_millis(10),
        )
        .expect("worker config"),
        Arc::new(InMemoryFileStore::default()),
    );
    assert!(
        run_until_job_done(
            &supervisor,
            &database,
            site_id,
            &context,
            TRASH_RETENTION_JOB.name,
        )
        .await,
        "form trash retention job should complete"
    );

    let mut transaction = database.begin(&context).await.expect("assert scope");
    let form_exists: bool =
        sqlx::query_scalar("select exists(select 1 from forms where site_id = $1 and id = $2)")
            .bind(site_id.into_uuid())
            .bind(form_id)
            .fetch_one(transaction.conn())
            .await
            .expect("form state");
    let submission_count: i64 = sqlx::query_scalar(
        "select count(*) from form_submissions where site_id = $1 and form_id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(form_id)
    .fetch_one(transaction.conn())
    .await
    .expect("submission state");
    assert!(!form_exists);
    assert_eq!(submission_count, 0);
    let resource_type: String = sqlx::query_scalar(
        "select resource_type from audit_events
          where site_id = $1 and action = 'trash.item.permanently_deleted'
          order by created_at desc limit 1",
    )
    .bind(site_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("retention audit");
    assert_eq!(resource_type, "Form");
    transaction.commit().await.expect("assert commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn trash_retention_worker_continues_after_a_full_batch() {
    let database_url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&database_url, 4)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");
    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");
    let seeded = seed_expired_trash_content_batch(
        &database,
        site_id,
        usize::try_from(MAX_TRASH_RETENTION_BATCH).expect("batch size") + 1,
    )
    .await;
    let context = SiteContext::public(site_id);
    let supervisor = WorkerSupervisor::new(
        database.clone(),
        [site_id],
        WorkerConfig::new(
            "test-trash-retention-batch-worker",
            30,
            std::time::Duration::from_millis(10),
        )
        .expect("worker config"),
        Arc::new(InMemoryFileStore::default()),
    );

    assert!(
        run_until_job_done(
            &supervisor,
            &database,
            site_id,
            &context,
            TRASH_RETENTION_JOB.name,
        )
        .await
    );

    let mut transaction = database.begin(&context).await.expect("assert scope");
    let remaining: i64 = sqlx::query_scalar(
        "select count(*) from content_entries
          where site_id = $1 and deleted_at is not null",
    )
    .bind(site_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("remaining trash");
    assert_eq!(remaining, 0);
    let retention_jobs: i64 =
        sqlx::query_scalar("select count(*) from jobs where site_id = $1 and kind = $2")
            .bind(site_id.into_uuid())
            .bind(TRASH_RETENTION_JOB.name)
            .fetch_one(transaction.conn())
            .await
            .expect("retention jobs");
    assert_eq!(retention_jobs, 2);
    let deletion_audits: i64 = sqlx::query_scalar(
        "select count(*) from audit_events
          where site_id = $1 and action = 'trash.item.permanently_deleted'",
    )
    .bind(site_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("retention audits");
    assert_eq!(deletion_audits, i64::try_from(seeded).expect("audit count"));
    transaction.commit().await.expect("assert commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn analytics_retention_worker_uses_site_policy_and_records_system_audit() {
    let database_url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&database_url, 4)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");
    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");

    let context = SiteContext::public(site_id);
    let old_event_id = AnalyticsEventId::new();
    let old_day = Utc::now().date_naive() - Duration::days(2);
    let mut transaction = database.begin(&context).await.expect("analytics scope");
    sqlx::query("insert into site_settings (site_id, name) values ($1, 'Analytics retention')")
        .bind(site_id.into_uuid())
        .execute(transaction.conn())
        .await
        .expect("site settings");
    sqlx::query(
        "update site_settings
            set analytics_raw_retention_days = 1,
                analytics_aggregate_retention_days = 1
          where site_id = $1",
    )
    .bind(site_id.into_uuid())
    .execute(transaction.conn())
    .await
    .expect("retention policy");
    sqlx::query(
        "insert into analytics_events
            (site_id, id, event_name, path, value, occurred_at, created_at)
         values ($1, $2, 'page_view', '/old', 1, $3, $3)",
    )
    .bind(site_id.into_uuid())
    .bind(old_event_id.into_uuid())
    .bind(Utc::now() - Duration::days(2))
    .execute(transaction.conn())
    .await
    .expect("old event");
    sqlx::query(
        "insert into analytics_daily
            (site_id, day, event_name, path, event_count, value_sum, value_min, value_max)
         values ($1, $2, 'page_view', '/old', 1, 1, 1, 1)",
    )
    .bind(site_id.into_uuid())
    .bind(old_day)
    .execute(transaction.conn())
    .await
    .expect("old aggregate");
    transaction.commit().await.expect("analytics seed commit");

    let supervisor = WorkerSupervisor::new(
        database.clone(),
        [site_id],
        WorkerConfig::new(
            "test-analytics-retention-worker",
            30,
            std::time::Duration::from_millis(10),
        )
        .expect("worker config"),
        Arc::new(InMemoryFileStore::default()),
    );
    // A poll discovers the ordinary housekeeping jobs as well. Retention is
    // intentionally lowest priority, so drive the same supervisor until its
    // discovered analytics job is the one that has completed.
    let mut retention_done = false;
    for _ in 0..=3 {
        assert!(supervisor.run_once(site_id).await.expect("retention run"));
        let mut check = database
            .begin(&context)
            .await
            .expect("retention check scope");
        let state: Option<String> = sqlx::query_scalar(
            "select state from jobs
               where site_id = $1 and kind = $2
               order by created_at desc limit 1",
        )
        .bind(site_id.into_uuid())
        .bind(ANALYTICS_RETENTION_JOB.name)
        .fetch_optional(check.conn())
        .await
        .expect("retention job state");
        check.commit().await.expect("retention check commit");
        if state.as_deref() == Some("done") {
            retention_done = true;
            break;
        }
    }
    assert!(retention_done, "analytics retention job should complete");

    let jobs = JobsService::new([ANALYTICS_RETENTION_JOB]);
    let mut transaction = database.begin(&context).await.expect("check scope");
    let event_exists: bool = sqlx::query_scalar(
        "select exists(
             select 1 from analytics_events where site_id = $1 and id = $2
         )",
    )
    .bind(site_id.into_uuid())
    .bind(old_event_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("event state");
    assert!(!event_exists);
    let aggregate_exists: bool = sqlx::query_scalar(
        "select exists(
             select 1 from analytics_daily
              where site_id = $1 and day = $2 and event_name = 'page_view' and path = '/old'
         )",
    )
    .bind(site_id.into_uuid())
    .bind(old_day)
    .fetch_one(transaction.conn())
    .await
    .expect("aggregate state");
    assert!(!aggregate_exists);
    let retention_jobs = jobs
        .list(
            &mut transaction,
            &mavi_jobs::JobListFilter {
                kind: Some(ANALYTICS_RETENTION_JOB.name.to_owned()),
                ..Default::default()
            },
        )
        .await
        .expect("retention jobs");
    assert_eq!(retention_jobs.items.len(), 1);
    assert_eq!(retention_jobs.items[0].state.as_str(), "done");
    let (actor_kind, actor_id): (String, Option<String>) = sqlx::query_as(
        "select actor_kind, actor_id from audit_events
          where site_id = $1 and action = 'analytics.retention.scheduled_pruned'
          order by created_at desc limit 1",
    )
    .bind(site_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("retention audit");
    assert_eq!(actor_kind, "system");
    assert_eq!(actor_id.as_deref(), Some("test-analytics-retention-worker"));
    transaction.commit().await.expect("check commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn media_cleanup_worker_removes_bytes_and_completes_the_receipt() {
    let database_url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&database_url, 4)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");
    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");

    let context = SiteContext::public(site_id);
    let store = Arc::new(InMemoryFileStore::default());
    let media = MediaService;
    let trash = TrashService;
    let jobs = JobsService::new([MEDIA_CLEANUP_JOB]);
    let mut transaction = database.begin(&context).await.expect("scope");
    let file = media
        .upload(
            &mut transaction,
            &context,
            store.as_ref(),
            "cleanup.png",
            FileVisibility::Private,
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR".to_vec(),
        )
        .await
        .expect("upload");
    transaction.commit().await.expect("upload commit");

    let mut transaction = database.begin(&context).await.expect("scope");
    media
        .trash(&mut transaction, &context, file.id)
        .await
        .expect("trash");
    let deletion = trash
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::File,
            file.id.into_uuid(),
        )
        .await
        .expect("permanent delete");
    let storage_key = deletion.file_storage_key.expect("storage key");
    let job_id = media
        .enqueue_cleanup_job(&mut transaction, &context, &jobs, file.id, &storage_key)
        .await
        .expect("cleanup job");
    transaction.commit().await.expect("delete commit");
    assert!(store.get(&context, &storage_key).await.is_ok());

    let supervisor = WorkerSupervisor::new(
        database.clone(),
        [site_id],
        WorkerConfig::new(
            "test-media-worker",
            30,
            std::time::Duration::from_millis(10),
        )
        .expect("worker config"),
        Arc::clone(&store) as Arc<dyn mavi_core::ports::FileStore>,
    );
    assert!(supervisor.run_once(site_id).await.expect("cleanup run"));
    assert!(store.get(&context, &storage_key).await.is_err());

    let mut transaction = database.begin(&context).await.expect("scope");
    let completed: bool = sqlx::query_scalar(
        "select completed_at is not null from media_cleanup_tasks
          where site_id = $1 and file_id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(file.id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("cleanup receipt");
    assert!(completed);
    let job = jobs.get(&mut transaction, job_id).await.expect("job");
    assert_eq!(job.state.as_str(), "done");
    let cleanup_audits: i64 = sqlx::query_scalar(
        "select count(*) from audit_events
          where site_id = $1 and action = 'media.file.cleanup_completed'",
    )
    .bind(site_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("cleanup audit");
    assert_eq!(cleanup_audits, 1);
    transaction.commit().await.expect("scope commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn media_orphan_worker_removes_only_unknown_generated_media_keys() {
    let database_url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&database_url, 4)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");
    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");

    let context = SiteContext::public(site_id);
    let store = Arc::new(InMemoryFileStore::default());
    let media = MediaService;
    let jobs = JobsService::new([MEDIA_ORPHAN_CLEANUP_JOB]);
    let mut transaction = database.begin(&context).await.expect("scope");
    let live = media
        .upload(
            &mut transaction,
            &context,
            store.as_ref(),
            "live.pdf",
            FileVisibility::Private,
            b"%PDF-1.7".to_vec(),
        )
        .await
        .expect("upload");
    transaction.commit().await.expect("upload commit");

    let mut transaction = database.begin(&context).await.expect("live key scope");
    let live_key: String =
        sqlx::query_scalar("select storage_key from media_files where site_id = $1 and id = $2")
            .bind(site_id.into_uuid())
            .bind(live.id.into_uuid())
            .fetch_one(transaction.conn())
            .await
            .expect("live key");
    transaction.commit().await.expect("live key commit");
    let orphan_key = "ab/0123456789abcdef0123456789abcd.png";
    store
        .put(&context, orphan_key, b"orphan".to_vec())
        .await
        .expect("orphan put");
    store
        .put(&context, "src/index.html", b"design".to_vec())
        .await
        .expect("design put");

    let supervisor = WorkerSupervisor::new(
        database.clone(),
        [site_id],
        WorkerConfig::new(
            "test-media-orphan-worker",
            30,
            std::time::Duration::from_millis(10),
        )
        .expect("worker config"),
        Arc::clone(&store) as Arc<dyn mavi_core::ports::FileStore>,
    );
    assert!(supervisor.run_once(site_id).await.expect("orphan run"));
    assert!(store.get(&context, &live_key).await.is_ok());
    assert!(store.get(&context, orphan_key).await.is_err());
    assert!(store.get(&context, "src/index.html").await.is_ok());

    let mut transaction = database.begin(&context).await.expect("audit scope");
    let orphan_audits: i64 = sqlx::query_scalar(
        "select count(*) from audit_events
          where site_id = $1 and action = 'media.orphans.cleaned'",
    )
    .bind(site_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("orphan audit");
    assert_eq!(orphan_audits, 1);
    let orphan_jobs = jobs
        .list(
            &mut transaction,
            &mavi_jobs::JobListFilter {
                kind: Some(MEDIA_ORPHAN_CLEANUP_JOB.name.to_owned()),
                ..Default::default()
            },
        )
        .await
        .expect("orphan jobs");
    assert_eq!(orphan_jobs.items.len(), 1);
    assert_eq!(orphan_jobs.items[0].state.as_str(), "done");
    transaction.commit().await.expect("audit commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn media_variant_worker_generates_all_presets_and_serves_public_bytes() {
    const ONE_PIXEL_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5,
        0x1c, 0x0c, 0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64,
        0xf8, 0x0f, 0x00, 0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xe3, 0x66, 0x00, 0x00, 0x00, 0x00,
        0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    let database_url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&database_url, 4)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");
    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");

    let context = SiteContext::public(site_id);
    let store = Arc::new(InMemoryFileStore::default());
    let media = MediaService;
    let mut transaction = database.begin(&context).await.expect("scope");
    let file = media
        .upload(
            &mut transaction,
            &context,
            store.as_ref(),
            "variant.png",
            FileVisibility::Public,
            ONE_PIXEL_PNG.to_vec(),
        )
        .await
        .expect("upload");
    transaction.commit().await.expect("upload commit");

    let supervisor = WorkerSupervisor::new(
        database.clone(),
        [site_id],
        WorkerConfig::new(
            "test-media-variant-worker",
            30,
            std::time::Duration::from_millis(10),
        )
        .expect("worker config"),
        Arc::clone(&store) as Arc<dyn mavi_core::ports::FileStore>,
    );
    for _ in 0..3 {
        assert!(supervisor.run_once(site_id).await.expect("variant run"));
    }

    let mut transaction = database.begin(&context).await.expect("scope");
    let variants = media
        .list_variants(
            &mut transaction,
            &context,
            file.id,
            &FileVariantListFilter::default(),
        )
        .await
        .expect("variants");
    assert_eq!(variants.items.len(), 3);
    assert_eq!(
        variants
            .items
            .iter()
            .map(|variant| variant.preset)
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([
            VariantPreset::Thumbnail,
            VariantPreset::Medium,
            VariantPreset::Large,
        ])
    );
    let (_, bytes) = media
        .read_variant_bytes(
            &mut transaction,
            &context,
            store.as_ref(),
            file.id,
            VariantPreset::Thumbnail,
            true,
        )
        .await
        .expect("public variant");
    assert!(bytes.starts_with(&[0xff, 0xd8, 0xff]));
    let variant_jobs: i64 = sqlx::query_scalar(
        "select count(*) from jobs where site_id = $1 and kind = $2 and state = 'done'",
    )
    .bind(site_id.into_uuid())
    .bind(MEDIA_VARIANT_JOB.name)
    .fetch_one(transaction.conn())
    .await
    .expect("variant jobs");
    assert_eq!(variant_jobs, 3);
    transaction.commit().await.expect("scope commit");

    let mut transaction = database.begin(&context).await.expect("delete scope");
    let original_key: String =
        sqlx::query_scalar("select storage_key from media_files where site_id = $1 and id = $2")
            .bind(site_id.into_uuid())
            .bind(file.id.into_uuid())
            .fetch_one(transaction.conn())
            .await
            .expect("original key");
    let variant_keys: Vec<String> = sqlx::query_scalar(
        "select storage_key from media_variants
          where site_id = $1 and source_file_id = $2
          order by id",
    )
    .bind(site_id.into_uuid())
    .bind(file.id.into_uuid())
    .fetch_all(transaction.conn())
    .await
    .expect("variant keys");
    media
        .trash(&mut transaction, &context, file.id)
        .await
        .expect("trash source");
    TrashService
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::File,
            file.id.into_uuid(),
        )
        .await
        .expect("permanent delete");
    transaction.commit().await.expect("delete commit");

    assert!(supervisor.run_once(site_id).await.expect("cleanup run"));
    assert!(store.get(&context, &original_key).await.is_err());
    for key in variant_keys {
        assert!(store.get(&context, &key).await.is_err());
    }
}
