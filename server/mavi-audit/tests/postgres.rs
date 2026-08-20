use mavi_audit::{AuditEntry, AuditExportFilter, AuditListFilter, AuditService};
use mavi_core::{MaviError, PageRequest, SiteContext, SiteId};
use mavi_storage::Database;
use serde_json::json;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn audit_receipts_are_site_scoped_and_cursor_listable() {
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

    let first_context = SiteContext::public(first_site);
    let service = AuditService;
    let mut transaction = database.begin(&first_context).await.expect("transaction");
    for action in ["content.created", "content.updated"] {
        service
            .record(
                &mut transaction,
                &first_context,
                &AuditEntry {
                    action: action.to_owned(),
                    resource_type: "Content".to_owned(),
                    resource_id: Some(uuid::Uuid::now_v7()),
                    payload: json!({"source": "audit-test"}),
                },
            )
            .await
            .expect("audit record");
    }
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let first_page = service
        .list(
            &mut transaction,
            &first_context,
            &AuditListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(1),
                },
                ..AuditListFilter::default()
            },
        )
        .await
        .expect("first audit page");
    assert_eq!(first_page.items.len(), 1);
    let cursor = first_page.next_cursor.clone().expect("audit cursor");
    let event = service
        .get(&mut transaction, &first_context, first_page.items[0].id)
        .await
        .expect("audit event");
    assert_eq!(event.actor_kind.as_str(), "public");
    assert_eq!(event.payload["source"], "audit-test");

    let update_blocked =
        sqlx::query("update audit_events set action = 'tampered' where site_id = $1")
            .bind(first_site.into_uuid())
            .execute(transaction.conn())
            .await;
    assert!(update_blocked.is_err());
    drop(transaction);

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let delete_blocked = sqlx::query("delete from audit_events where site_id = $1")
        .bind(first_site.into_uuid())
        .execute(transaction.conn())
        .await;
    assert!(delete_blocked.is_err());
    drop(transaction);

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let second_page = service
        .list(
            &mut transaction,
            &first_context,
            &AuditListFilter {
                page: PageRequest {
                    after: Some(cursor),
                    limit: Some(1),
                },
                ..AuditListFilter::default()
            },
        )
        .await
        .expect("second audit page");
    assert_eq!(second_page.items.len(), 1);
    assert!(second_page.next_cursor.is_none());

    let export = service
        .export(
            &mut transaction,
            &first_context,
            &AuditExportFilter {
                limit: Some(1),
                ..AuditExportFilter::default()
            },
        )
        .await
        .expect("bounded audit export");
    assert_eq!(export.format, "mavi.audit.export");
    assert_eq!(export.version, 1);
    assert_eq!(export.site_id, first_site);
    assert_eq!(export.items.len(), 1);
    assert!(export.truncated);

    let invalid_export = service
        .export(
            &mut transaction,
            &first_context,
            &AuditExportFilter {
                limit: Some(10_001),
                ..AuditExportFilter::default()
            },
        )
        .await
        .expect_err("oversized export");
    assert!(matches!(
        invalid_export,
        MaviError::Validation { code, .. } if code == "audit_export_filter_invalid"
    ));

    let invalid = service
        .list(
            &mut transaction,
            &first_context,
            &AuditListFilter {
                action: Some("x".repeat(161)),
                ..AuditListFilter::default()
            },
        )
        .await
        .expect_err("invalid filter");
    assert!(
        matches!(invalid, MaviError::Validation { code, .. } if code == "audit_filter_invalid")
    );
    transaction.commit().await.expect("commit");

    let second_context = SiteContext::public(second_site);
    let mut transaction = database.begin(&second_context).await.expect("transaction");
    let isolated = service
        .list(
            &mut transaction,
            &second_context,
            &AuditListFilter::default(),
        )
        .await
        .expect("isolated audit page");
    assert!(isolated.items.is_empty());
    let cross_site = service
        .get(&mut transaction, &second_context, event.id)
        .await
        .expect_err("cross-site audit read");
    assert!(matches!(cross_site, MaviError::NotFound { .. }));
    transaction.commit().await.expect("commit");
}
