use std::env;

use chrono::{Duration, Utc};
use mavi_analytics::{
    ANALYTICS_RETENTION_JOB, AnalyticsEventBatch, AnalyticsEventInput, AnalyticsService,
    DailyListFilter, EventListFilter, PruneAnalytics,
};
use mavi_core::{PageRequest, SiteContext, SiteId};
use mavi_jobs::{JobListFilter, JobsService};
use mavi_storage::Database;

fn database_url() -> Option<String> {
    env::var("TEST_DATABASE_URL").ok()
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn analytics_are_bounded_aggregated_cursor_only_and_site_scoped() {
    let url = database_url().expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 4).await.expect("database");
    database.migrate().await.expect("migrations");
    let first_site = SiteId::new();
    let second_site = SiteId::new();
    database.ensure_site(first_site).await.expect("first site");
    database
        .ensure_site(second_site)
        .await
        .expect("second site");

    let analytics = AnalyticsService;
    let first_context = SiteContext::public(first_site);
    let occurred_at = Utc::now() - Duration::minutes(1);
    let mut tx = database.begin(&first_context).await.expect("record scope");
    let receipt = analytics
        .record_batch(
            &mut tx,
            &first_context,
            &AnalyticsEventBatch {
                events: vec![
                    AnalyticsEventInput {
                        event_name: "page_view".to_owned(),
                        path: "/home".to_owned(),
                        value: None,
                        occurred_at: Some(occurred_at),
                    },
                    AnalyticsEventInput {
                        event_name: "checkout_started".to_owned(),
                        path: "/checkout".to_owned(),
                        value: Some(10),
                        occurred_at: Some(occurred_at),
                    },
                    AnalyticsEventInput {
                        event_name: "page_view".to_owned(),
                        path: "/home".to_owned(),
                        value: Some(2),
                        occurred_at: Some(occurred_at),
                    },
                ],
            },
        )
        .await
        .expect("record");
    assert_eq!(receipt.accepted, 3);

    let raw_page = analytics
        .list_events(
            &mut tx,
            &EventListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(1),
                },
                event_name: None,
                path: None,
            },
        )
        .await
        .expect("raw cursor list");
    assert_eq!(raw_page.items.len(), 1);
    assert!(raw_page.next_cursor.is_some());
    let daily_page = analytics
        .list_daily(
            &mut tx,
            &DailyListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(1),
                },
                event_name: None,
                path: None,
            },
        )
        .await
        .expect("daily cursor list");
    assert_eq!(daily_page.items.len(), 1);
    assert!(daily_page.next_cursor.is_some());
    assert_eq!(daily_page.items[0].event_count, 1);
    let page_view_daily = analytics
        .list_daily(
            &mut tx,
            &DailyListFilter {
                page: PageRequest::default(),
                event_name: Some("page_view".to_owned()),
                path: None,
            },
        )
        .await
        .expect("page view aggregate");
    assert_eq!(page_view_daily.items[0].event_count, 2);
    assert_eq!(page_view_daily.items[0].value_sum, 2);

    let receipt = analytics
        .prune(
            &mut tx,
            &first_context,
            &PruneAnalytics {
                raw_days: 1,
                aggregate_days: 1,
            },
        )
        .await
        .expect("retention");
    assert_eq!(receipt.deleted_events, 0);
    assert_eq!(receipt.deleted_aggregates, 0);
    tx.commit().await.expect("record commit");

    let second_context = SiteContext::public(second_site);
    let mut tx = database
        .begin(&second_context)
        .await
        .expect("isolation scope");
    assert!(
        analytics
            .list_events(&mut tx, &EventListFilter::default())
            .await
            .expect("second raw list")
            .items
            .is_empty()
    );
    assert!(
        analytics
            .list_daily(&mut tx, &DailyListFilter::default())
            .await
            .expect("second daily list")
            .items
            .is_empty()
    );
    tx.commit().await.expect("isolation commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn analytics_retention_enqueue_is_idempotent_per_site_and_day() {
    let url = database_url().expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 4).await.expect("database");
    database.migrate().await.expect("migrations");
    let first_site = SiteId::new();
    let second_site = SiteId::new();
    database.ensure_site(first_site).await.expect("first site");
    database
        .ensure_site(second_site)
        .await
        .expect("second site");

    let analytics = AnalyticsService;
    let now = Utc::now();
    let first_context = SiteContext::system(
        first_site,
        "analytics-retention-test".to_owned(),
        mavi_core::RequestId::new(),
    );
    let second_context = SiteContext::system(
        second_site,
        "analytics-retention-test".to_owned(),
        mavi_core::RequestId::new(),
    );

    let first_job = {
        let mut tx = database.begin(&first_context).await.expect("first scope");
        let jobs = JobsService::new([ANALYTICS_RETENTION_JOB]);
        let first = analytics
            .enqueue_retention_job(&mut tx, &first_context, &jobs, now)
            .await
            .expect("first retention job");
        let second = analytics
            .enqueue_retention_job(&mut tx, &first_context, &jobs, now)
            .await
            .expect("idempotent retention job");
        assert_eq!(first, second);
        tx.commit().await.expect("first commit");
        first
    };

    let second_job = {
        let mut tx = database.begin(&second_context).await.expect("second scope");
        let jobs = JobsService::new([ANALYTICS_RETENTION_JOB]);
        let job = analytics
            .enqueue_retention_job(&mut tx, &second_context, &jobs, now)
            .await
            .expect("second retention job");
        tx.commit().await.expect("second commit");
        job
    };
    assert_ne!(first_job, second_job);

    let jobs = JobsService::new([ANALYTICS_RETENTION_JOB]);
    let mut first_tx = database
        .begin(&first_context)
        .await
        .expect("first list scope");
    let first_page = jobs
        .list(
            &mut first_tx,
            &JobListFilter {
                kind: Some(ANALYTICS_RETENTION_JOB.name.to_owned()),
                ..Default::default()
            },
        )
        .await
        .expect("first jobs");
    assert_eq!(first_page.items.len(), 1);
    assert_eq!(first_page.items[0].id, first_job);
    first_tx.commit().await.expect("first list commit");

    let mut second_tx = database
        .begin(&second_context)
        .await
        .expect("second list scope");
    let second_page = jobs
        .list(
            &mut second_tx,
            &JobListFilter {
                kind: Some(ANALYTICS_RETENTION_JOB.name.to_owned()),
                ..Default::default()
            },
        )
        .await
        .expect("second jobs");
    assert_eq!(second_page.items.len(), 1);
    assert_eq!(second_page.items[0].id, second_job);
    second_tx.commit().await.expect("second list commit");
}
