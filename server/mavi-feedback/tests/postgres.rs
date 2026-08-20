use mavi_core::{
    Action, Caller, Capability, Grant, Grants, PageRequest, PersonId, RequestId, SiteContext,
    SiteId,
};
use mavi_feedback::{CreateReport, FeedbackService, ReportKind, ReportListFilter};
use mavi_storage::Database;
use serde_json::json;

fn account_context(site_id: SiteId) -> SiteContext {
    SiteContext::with_caller(
        site_id,
        Caller::Account {
            person_id: PersonId::new(),
            session_id: None,
            grants: Grants::new([Grant::new(Capability::Feedback, Action::Write)]),
        },
        RequestId::new(),
    )
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn feedback_is_site_scoped_cursor_listable_and_audited() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2).await.expect("database");
    database.migrate().await.expect("migrations");
    let first_site = SiteId::new();
    let second_site = SiteId::new();
    database.ensure_site(first_site).await.expect("first site");
    database
        .ensure_site(second_site)
        .await
        .expect("second site");

    let service = FeedbackService;
    let context = account_context(first_site);
    let mut transaction = database.begin(&context).await.expect("scope");
    service
        .create(
            &mut transaction,
            &context,
            &CreateReport {
                kind: ReportKind::Broken,
                title: "Broken test report".to_owned(),
                body: "Details".to_owned(),
                context: json!({"screen":"test"}),
            },
        )
        .await
        .expect("create report");
    service
        .create(
            &mut transaction,
            &context,
            &CreateReport {
                kind: ReportKind::Wanted,
                title: "Wanted test report".to_owned(),
                body: String::new(),
                context: json!({}),
            },
        )
        .await
        .expect("create second report");

    let page = service
        .list(
            &mut transaction,
            &ReportListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(1),
                },
                state: None,
            },
        )
        .await
        .expect("first page");
    assert_eq!(page.items.len(), 1);
    let cursor = page.next_cursor.expect("cursor");
    let next = service
        .list(
            &mut transaction,
            &ReportListFilter {
                page: PageRequest {
                    after: Some(cursor),
                    limit: Some(1),
                },
                state: None,
            },
        )
        .await
        .expect("second page");
    assert_eq!(next.items.len(), 1);
    transaction.commit().await.expect("commit");

    let second_context = account_context(second_site);
    let mut transaction = database.begin(&second_context).await.expect("second scope");
    let isolated = service
        .list(&mut transaction, &ReportListFilter::default())
        .await
        .expect("isolated list");
    assert!(isolated.items.is_empty());
    transaction.commit().await.expect("second commit");
}
