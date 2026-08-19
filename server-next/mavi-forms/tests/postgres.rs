use std::env;

use chrono::{Duration, Utc};
use mavi_core::{FormSubmissionId, MaviError, PageRequest, SiteContext, SiteId};
use mavi_forms::{
    CreateForm, FORM_RETENTION_JOB, FormField, FormFieldKind, FormListFilter, FormService,
    SubmissionListFilter, SubmitForm,
};
use mavi_jobs::JobsService;
use mavi_storage::Database;
use serde_json::{Map, Value, json};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn forms_declarations_submissions_and_rls_are_site_scoped() {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2).await.expect("database");
    database.migrate().await.expect("migrations");

    let first_site = SiteId::new();
    let second_site = SiteId::new();
    database.ensure_site(first_site).await.expect("first site");
    database
        .ensure_site(second_site)
        .await
        .expect("second site");

    let service = FormService;
    let first_context = SiteContext::public(first_site);
    let fields = vec![
        FormField {
            key: "name".to_owned(),
            label: "Name".to_owned(),
            required: true,
            kind: FormFieldKind::Text,
            options: Vec::new(),
        },
        FormField {
            key: "email".to_owned(),
            label: "Email".to_owned(),
            required: true,
            kind: FormFieldKind::Email,
            options: Vec::new(),
        },
        FormField {
            key: "topic".to_owned(),
            label: "Topic".to_owned(),
            required: true,
            kind: FormFieldKind::Choice,
            options: vec!["sales".to_owned(), "support".to_owned()],
        },
    ];

    let mut transaction = database.begin(&first_context).await.expect("first scope");
    let first_form = service
        .create(
            &mut transaction,
            &first_context,
            &CreateForm {
                slug: "contact".to_owned(),
                name: "Contact us".to_owned(),
                fields: fields.clone(),
                kept_days: Some(30),
            },
        )
        .await
        .expect("first form");
    let second_form = service
        .create(
            &mut transaction,
            &first_context,
            &CreateForm {
                slug: "feedback".to_owned(),
                name: "Feedback".to_owned(),
                fields: Vec::new(),
                kept_days: None,
            },
        )
        .await
        .expect("second form");
    assert_eq!(second_form.kept_days, 365);

    let first_page = service
        .list(
            &mut transaction,
            &first_context,
            &FormListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(1),
                },
            },
        )
        .await
        .expect("first cursor page");
    assert_eq!(first_page.items.len(), 1);
    let cursor = first_page.next_cursor.clone().expect("form cursor");
    let second_page = service
        .list(
            &mut transaction,
            &first_context,
            &FormListFilter {
                page: PageRequest {
                    after: Some(cursor),
                    limit: Some(1),
                },
            },
        )
        .await
        .expect("second cursor page");
    assert_eq!(second_page.items.len(), 1);
    assert!(second_page.next_cursor.is_none());

    let public = service
        .public_get(&mut transaction, &first_context, "contact")
        .await
        .expect("public form");
    assert_eq!(public.slug, "contact");
    assert_eq!(public.fields, fields);

    let invalid_answers = serde_json::from_value::<Map<String, Value>>(json!({
        "name": "Visitor",
        "email": "visitor@example.test",
        "topic": "billing"
    }))
    .expect("invalid answers");
    assert!(matches!(
        service
            .submit(
                &mut transaction,
                &first_context,
                "contact",
                &SubmitForm {
                    answers: invalid_answers,
                },
            )
            .await,
        Err(MaviError::Validation { .. })
    ));

    let answers = serde_json::from_value::<Map<String, Value>>(json!({
        "name": "Visitor",
        "email": "visitor@example.test",
        "topic": "support"
    }))
    .expect("answers");
    let receipt = service
        .submit(
            &mut transaction,
            &first_context,
            "contact",
            &SubmitForm { answers },
        )
        .await
        .expect("submission");

    let unread = service
        .list_submissions(
            &mut transaction,
            &first_context,
            first_form.id,
            &SubmissionListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(10),
                },
                unread: true,
            },
        )
        .await
        .expect("unread submissions");
    assert_eq!(unread.items.len(), 1);
    assert_eq!(unread.items[0].id, receipt.id);

    let seen = service
        .mark_read(&mut transaction, &first_context, first_form.id)
        .await
        .expect("mark read");
    assert_eq!(seen.seen, 1);
    let unread_after = service
        .list_submissions(
            &mut transaction,
            &first_context,
            first_form.id,
            &SubmissionListFilter {
                page: PageRequest::default(),
                unread: true,
            },
        )
        .await
        .expect("empty unread submissions");
    assert!(unread_after.items.is_empty());

    service
        .delete_submission(&mut transaction, &first_context, receipt.id)
        .await
        .expect("delete submission");
    let audit_count: i64 = sqlx::query_scalar(
        "select count(*) from audit_events where site_id = $1 and action like 'forms.%'",
    )
    .bind(first_site.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("forms audit count");
    assert_eq!(audit_count, 5);
    transaction.commit().await.expect("first commit");

    let second_context = SiteContext::public(second_site);
    let mut second_transaction = database.begin(&second_context).await.expect("second scope");
    let same_slug = service
        .create(
            &mut second_transaction,
            &second_context,
            &CreateForm {
                slug: "contact".to_owned(),
                name: "Second site contact".to_owned(),
                fields: Vec::new(),
                kept_days: None,
            },
        )
        .await
        .expect("same slug on second site");
    assert_ne!(same_slug.id, first_form.id);
    let second_site_forms = service
        .list(
            &mut second_transaction,
            &second_context,
            &FormListFilter::default(),
        )
        .await
        .expect("second site forms");
    assert_eq!(second_site_forms.items.len(), 1);
    assert!(matches!(
        service
            .get(&mut second_transaction, &second_context, first_form.id)
            .await,
        Err(MaviError::NotFound { .. })
    ));
    second_transaction.commit().await.expect("second commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn retention_is_idempotent_site_scoped_and_audited() {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2).await.expect("database");
    database.migrate().await.expect("migrations");

    let first_site = SiteId::new();
    let second_site = SiteId::new();
    database.ensure_site(first_site).await.expect("first site");
    database
        .ensure_site(second_site)
        .await
        .expect("second site");
    let first_context = SiteContext::public(first_site);
    let second_context = SiteContext::public(second_site);
    let service = FormService;
    let jobs = JobsService::new([FORM_RETENTION_JOB]);
    let now = Utc::now();

    let first_form = {
        let mut transaction = database.begin(&first_context).await.expect("first scope");
        let form = service
            .create(
                &mut transaction,
                &first_context,
                &CreateForm {
                    slug: "retention".to_owned(),
                    name: "Retention".to_owned(),
                    fields: Vec::new(),
                    kept_days: Some(1),
                },
            )
            .await
            .expect("first form");
        transaction.commit().await.expect("first form commit");
        form
    };
    let second_form = {
        let mut transaction = database.begin(&second_context).await.expect("second scope");
        let form = service
            .create(
                &mut transaction,
                &second_context,
                &CreateForm {
                    slug: "retention".to_owned(),
                    name: "Retention".to_owned(),
                    fields: Vec::new(),
                    kept_days: Some(1),
                },
            )
            .await
            .expect("second form");
        transaction.commit().await.expect("second form commit");
        form
    };

    let first_old = FormSubmissionId::new();
    let first_fresh = FormSubmissionId::new();
    let second_old = FormSubmissionId::new();
    {
        let mut transaction = database.begin(&first_context).await.expect("first inserts");
        for (id, created_at) in [
            (first_old, now - Duration::days(2)),
            (first_fresh, now - Duration::hours(1)),
        ] {
            sqlx::query(
                "insert into form_submissions (site_id, id, form_id, answers, created_at)
                 values ($1, $2, $3, '{}'::jsonb, $4)",
            )
            .bind(first_site.into_uuid())
            .bind(id.into_uuid())
            .bind(first_form.id.into_uuid())
            .bind(created_at)
            .execute(transaction.conn())
            .await
            .expect("first submission");
        }
        transaction.commit().await.expect("first inserts commit");
    }
    {
        let mut transaction = database
            .begin(&second_context)
            .await
            .expect("second inserts");
        sqlx::query(
            "insert into form_submissions (site_id, id, form_id, answers, created_at)
             values ($1, $2, $3, '{}'::jsonb, $4)",
        )
        .bind(second_site.into_uuid())
        .bind(second_old.into_uuid())
        .bind(second_form.id.into_uuid())
        .bind(now - Duration::days(2))
        .execute(transaction.conn())
        .await
        .expect("second submission");
        transaction.commit().await.expect("second inserts commit");
    }

    let (job_id, deleted) = {
        let mut transaction = database
            .begin(&first_context)
            .await
            .expect("retention scope");
        let job_id = service
            .enqueue_retention_job(&mut transaction, &first_context, &jobs, now)
            .await
            .expect("retention job");
        assert_eq!(
            service
                .enqueue_retention_job(&mut transaction, &first_context, &jobs, now)
                .await
                .expect("idempotent retention job"),
            job_id
        );
        let deleted = service
            .prune_expired_submissions(
                &mut transaction,
                &first_context,
                now,
                now.timestamp().div_euclid(24 * 60 * 60),
            )
            .await
            .expect("prune");
        transaction.commit().await.expect("retention commit");
        (job_id, deleted)
    };
    assert_eq!(deleted, 1);

    let mut transaction = database.begin(&first_context).await.expect("first check");
    let first_deleted: bool = sqlx::query_scalar(
        "select deleted_at is not null from form_submissions where site_id = $1 and id = $2",
    )
    .bind(first_site.into_uuid())
    .bind(first_old.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("first deleted state");
    let first_answers_redacted: bool = sqlx::query_scalar(
        "select answers = '{}'::jsonb from form_submissions where site_id = $1 and id = $2",
    )
    .bind(first_site.into_uuid())
    .bind(first_old.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("first answers state");
    let first_fresh_deleted: bool = sqlx::query_scalar(
        "select deleted_at is not null from form_submissions where site_id = $1 and id = $2",
    )
    .bind(first_site.into_uuid())
    .bind(first_fresh.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("fresh deleted state");
    let audit_count: i64 = sqlx::query_scalar(
        "select count(*) from audit_events
          where site_id = $1 and action = 'forms.submissions.retention_pruned'",
    )
    .bind(first_site.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("retention audit");
    assert!(first_deleted);
    assert!(first_answers_redacted);
    assert!(!first_fresh_deleted);
    assert_eq!(audit_count, 1);
    let job = jobs
        .get(&mut transaction, job_id)
        .await
        .expect("retention job state");
    assert_eq!(job.state.as_str(), "ready");
    transaction.commit().await.expect("first check commit");

    let mut transaction = database.begin(&second_context).await.expect("second check");
    let second_deleted: bool = sqlx::query_scalar(
        "select deleted_at is not null from form_submissions where site_id = $1 and id = $2",
    )
    .bind(second_site.into_uuid())
    .bind(second_old.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("second deleted state");
    assert!(!second_deleted);
    transaction.commit().await.expect("second check commit");
}
