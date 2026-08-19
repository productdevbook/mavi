use std::{env, sync::Arc};

use chrono::{Duration, Utc};
use mavi_content::{
    ContentService, CreateContent, Publication, PublicationInput, SCHEDULED_PUBLISH_JOB,
    ScheduledPublishJob,
};
use mavi_core::{SiteContext, SiteId, ports::FileStore};
use mavi_files::InMemoryFileStore;
use mavi_jobs::JobsService;
use mavi_media::{FileVisibility, MEDIA_CLEANUP_JOB, MediaService};
use mavi_storage::Database;
use mavi_trash::{TrashKind, TrashService};
use mavi_worker::{WorkerConfig, WorkerSupervisor};
use serde_json::to_value;

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
