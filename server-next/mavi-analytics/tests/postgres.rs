use std::env;

use chrono::{Duration, Utc};
use mavi_analytics::{
    AnalyticsEventBatch, AnalyticsEventInput, AnalyticsService, DailyListFilter, EventListFilter,
    PruneAnalytics,
};
use mavi_core::{PageRequest, SiteContext, SiteId};
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
