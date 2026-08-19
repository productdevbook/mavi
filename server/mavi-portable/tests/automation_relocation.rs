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
async fn automation_relocation_preserves_domain_state_and_resets_transient_claims() {
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

    let now = Utc::now();
    let course_id = Uuid::now_v7();
    let module_id = Uuid::now_v7();
    let lesson_id = Uuid::now_v7();
    let student_id = Uuid::now_v7();
    let enrollment_id = Uuid::now_v7();
    let job_id = Uuid::now_v7();
    let flow_id = Uuid::now_v7();
    let flow_step_id = Uuid::now_v7();
    let run_id = Uuid::now_v7();
    let board_id = Uuid::now_v7();
    let list_id = Uuid::now_v7();
    let card_id = Uuid::now_v7();
    let comment_id = Uuid::now_v7();
    let activity_id = Uuid::now_v7();
    let event_id = Uuid::now_v7();
    let context = SiteContext::public(source_site);
    let files = InMemoryFileStore::default();
    let portable = PortableService;
    let mut source_tx = database.begin(&context).await.expect("source scope");

    sqlx::query(
        "insert into site_settings (site_id, name, timezone)
         values ($1, 'Automation source', 'UTC')",
    )
    .bind(source_site.into_uuid())
    .execute(source_tx.conn())
    .await
    .expect("source settings");
    sqlx::query(
        "insert into site_languages (site_id, tag, name, is_default)
         values ($1, 'en', 'English', true)",
    )
    .bind(source_site.into_uuid())
    .execute(source_tx.conn())
    .await
    .expect("source language");

    sqlx::query(
        "insert into courses (site_id, id, slug, title, state, created_at, updated_at)
         values ($1, $2, 'rust-course', 'Rust course', 'open', $3, $3)",
    )
    .bind(source_site.into_uuid())
    .bind(course_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("course");
    sqlx::query(
        "insert into course_modules (site_id, id, course_id, title, position, created_at, updated_at)
         values ($1, $2, $3, 'Basics', 0, $4, $4)",
    )
    .bind(source_site.into_uuid())
    .bind(module_id)
    .bind(course_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("course module");
    sqlx::query(
        "insert into course_lessons
            (site_id, id, module_id, title, body, position, created_at, updated_at)
         values ($1, $2, $3, 'Ownership', 'Borrowing', 0, $4, $4)",
    )
    .bind(source_site.into_uuid())
    .bind(lesson_id)
    .bind(module_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("course lesson");
    sqlx::query(
        "insert into course_students
            (site_id, id, email, name, password_hash, standing, created_at, updated_at)
         values ($1, $2, 'student@example.test', 'Student', '$argon2id$v=19$hash', 'learning', $3, $3)",
    )
    .bind(source_site.into_uuid())
    .bind(student_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("course student");
    sqlx::query(
        "insert into course_student_sessions
            (site_id, id, student_id, token_hash, expires_at, created_at)
         values ($1, $2, $3, $4, $5, $5)",
    )
    .bind(source_site.into_uuid())
    .bind(Uuid::now_v7())
    .bind(student_id)
    .bind(vec![4_u8; 32])
    .bind(now + chrono::Duration::days(1))
    .execute(source_tx.conn())
    .await
    .expect("student session");
    sqlx::query(
        "insert into course_enrollments
            (site_id, id, course_id, student_id, started_at, created_at)
         values ($1, $2, $3, $4, $5, $5)",
    )
    .bind(source_site.into_uuid())
    .bind(enrollment_id)
    .bind(course_id)
    .bind(student_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("enrollment");
    sqlx::query(
        "insert into course_progress (site_id, student_id, lesson_id, completed_at)
         values ($1, $2, $3, $4)",
    )
    .bind(source_site.into_uuid())
    .bind(student_id)
    .bind(lesson_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("progress");

    sqlx::query(
        "insert into jobs
            (site_id, id, kind, payload, state, run_at, claimed_until, claimed_by,
             claim_token, attempts, idempotency_key, created_at)
         values ($1, $2, 'automation.flow.step', $3, 'running', $4, $5, 'old-worker',
                 $2, 2, 'flow-step-1', $4)",
    )
    .bind(source_site.into_uuid())
    .bind(job_id)
    .bind(json!({"run_id": run_id, "position": 0}))
    .bind(now)
    .bind(now + chrono::Duration::minutes(5))
    .execute(source_tx.conn())
    .await
    .expect("leased job");
    sqlx::query(
        "insert into automation_flows
            (site_id, id, name, trigger, enabled, version, created_at, updated_at)
         values ($1, $2, 'Welcome flow', 'content_published', true, 1, $3, $3)",
    )
    .bind(source_site.into_uuid())
    .bind(flow_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("flow");
    sqlx::query(
        "insert into automation_flow_steps
            (site_id, id, flow_id, position, kind, config)
         values ($1, $2, $3, 0, 'wait', $4)",
    )
    .bind(source_site.into_uuid())
    .bind(flow_step_id)
    .bind(flow_id)
    .bind(json!({"seconds": 30}))
    .execute(source_tx.conn())
    .await
    .expect("flow step");
    sqlx::query(
        "insert into automation_runs
            (site_id, id, flow_id, trigger, source_key, event, definition, state,
             current_position, retry_count, started_at, updated_at)
         values ($1, $2, $3, 'content_published', 'content-1', $4, $5, 'running', 0, 1, $6, $6)",
    )
    .bind(source_site.into_uuid())
    .bind(run_id)
    .bind(flow_id)
    .bind(json!({"content_id": Uuid::now_v7()}))
    .bind(json!([{"kind": "wait", "config": {"seconds": 30}}]))
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("flow run");

    sqlx::query(
        "insert into boards
            (site_id, id, name, description, created_at, updated_at)
         values ($1, $2, 'Migration board', 'Board', $3, $3)",
    )
    .bind(source_site.into_uuid())
    .bind(board_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("board");
    sqlx::query(
        "insert into board_lists
            (site_id, id, board_id, name, position, created_at, updated_at)
         values ($1, $2, $3, 'Todo', 0, $4, $4)",
    )
    .bind(source_site.into_uuid())
    .bind(list_id)
    .bind(board_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("board list");
    sqlx::query(
        "insert into board_cards
            (site_id, id, board_id, list_id, title, position, created_at, updated_at)
         values ($1, $2, $3, $4, 'Relocate me', 0, $5, $5)",
    )
    .bind(source_site.into_uuid())
    .bind(card_id)
    .bind(board_id)
    .bind(list_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("board card");
    sqlx::query(
        "insert into board_comments
            (site_id, id, board_id, card_id, body, created_at)
         values ($1, $2, $3, $4, 'Keep history', $5)",
    )
    .bind(source_site.into_uuid())
    .bind(comment_id)
    .bind(board_id)
    .bind(card_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("board comment");
    sqlx::query(
        "insert into board_activity
            (site_id, id, board_id, card_id, kind, actor_kind, detail, created_at)
         values ($1, $2, $3, $4, 'card_created', 'assistant', $5, $6)",
    )
    .bind(source_site.into_uuid())
    .bind(activity_id)
    .bind(board_id)
    .bind(card_id)
    .bind(json!({"source": "relocation"}))
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("board activity");

    sqlx::query(
        "insert into analytics_events
            (site_id, id, event_name, path, value, occurred_at, created_at)
         values ($1, $2, 'page_view', '/courses', 3, $3, $3)",
    )
    .bind(source_site.into_uuid())
    .bind(event_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("analytics event");
    sqlx::query(
        "insert into analytics_daily
            (site_id, day, event_name, path, event_count, value_sum, value_min, value_max)
         values ($1, $2, 'page_view', '/courses', 1, 3, 3, 3)",
    )
    .bind(source_site.into_uuid())
    .bind(now.date_naive())
    .execute(source_tx.conn())
    .await
    .expect("analytics daily");

    let mut relocation = portable
        .export_for_relocation(&mut source_tx, &context, &files)
        .await
        .expect("relocation export");
    assert_eq!(relocation.courses.progress.len(), 1);
    assert_eq!(relocation.jobs.jobs[0].state, mavi_jobs::JobState::Ready);
    assert_eq!(relocation.flows.runs.len(), 1);
    assert_eq!(relocation.boards.activity.len(), 1);
    assert_eq!(relocation.analytics.events.len(), 1);
    source_tx.commit().await.expect("source commit");

    relocation.bundle.manifest.source_site_id = target_site;
    relocation.audit.source_site_id = target_site;
    relocation.trash.source_site_id = target_site;
    relocation.forms.source_site_id = target_site;
    relocation.mail.source_site_id = target_site;
    relocation.shop.source_site_id = target_site;
    relocation.courses.source_site_id = target_site;
    relocation.jobs.source_site_id = target_site;
    relocation.flows.source_site_id = target_site;
    relocation.boards.source_site_id = target_site;
    relocation.analytics.source_site_id = target_site;

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

    let session_count: i64 =
        sqlx::query_scalar("select count(*) from course_student_sessions where site_id = $1")
            .bind(target_site.into_uuid())
            .fetch_one(target_tx.conn())
            .await
            .expect("target sessions");
    assert_eq!(session_count, 0);
    let (state, claimed_by): (String, Option<String>) =
        sqlx::query_as("select state, claimed_by from jobs where site_id = $1 and id = $2")
            .bind(target_site.into_uuid())
            .bind(job_id)
            .fetch_one(target_tx.conn())
            .await
            .expect("target job");
    assert_eq!(state, "ready");
    assert!(claimed_by.is_none());

    let relocated = portable
        .export_for_relocation(&mut target_tx, &target_context, &files)
        .await
        .expect("target export");
    assert_eq!(relocated.courses, relocation.courses);
    assert_eq!(relocated.jobs, relocation.jobs);
    assert_eq!(relocated.flows, relocation.flows);
    assert_eq!(relocated.boards, relocation.boards);
    assert_eq!(relocated.analytics, relocation.analytics);
}
