use mavi_core::{MaviError, PageRequest, SiteContext, SiteId, ports::FileStore};
use mavi_files::InMemoryFileStore;
use mavi_media::{FileListFilter, FileVisibility, MediaService};
use mavi_storage::Database;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn media_metadata_and_audit_are_site_scoped_and_binary_cleanup_is_retryable() {
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
    let second_context = SiteContext::public(second_site);
    let store = InMemoryFileStore::default();
    let service = MediaService;

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let first = service
        .upload(
            &mut transaction,
            &first_context,
            &store,
            "first.png",
            FileVisibility::Private,
            PNG.to_vec(),
        )
        .await
        .expect("first upload");
    let second = service
        .upload(
            &mut transaction,
            &first_context,
            &store,
            "second.png",
            FileVisibility::Public,
            PNG.to_vec(),
        )
        .await
        .expect("second upload");
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let page = service
        .list(
            &mut transaction,
            &first_context,
            &FileListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(1),
                },
                kind: None,
            },
        )
        .await
        .expect("list");
    assert_eq!(page.items.len(), 1);
    assert!(page.next_cursor.is_some());
    let next_filter = FileListFilter {
        page: PageRequest {
            after: page.next_cursor,
            limit: Some(1),
        },
        kind: None,
    };
    let next = service
        .list(&mut transaction, &first_context, &next_filter)
        .await
        .expect("next page");
    assert_eq!(next.items.len(), 1);
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&second_context).await.expect("transaction");
    let cross_site = service
        .get(&mut transaction, &second_context, first.id)
        .await;
    assert!(matches!(cross_site, Err(MaviError::NotFound { .. })));
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let storage_key: String =
        sqlx::query_scalar("select storage_key from media_files where site_id = $1 and id = $2")
            .bind(first_site.into_uuid())
            .bind(first.id.into_uuid())
            .fetch_one(transaction.conn())
            .await
            .expect("storage key");
    service
        .trash(&mut transaction, &first_context, first.id)
        .await
        .expect("trash metadata");
    transaction.commit().await.expect("commit");
    assert!(store.get(&first_context, &storage_key).await.is_ok());

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let missing = service
        .get(&mut transaction, &first_context, first.id)
        .await;
    assert!(matches!(missing, Err(MaviError::NotFound { .. })));
    let audit_actions: Vec<String> = sqlx::query_scalar(
        "select action from audit_events where site_id = $1 and resource_id = $2 order by action",
    )
    .bind(first_site.into_uuid())
    .bind(first.id.into_uuid())
    .fetch_all(transaction.conn())
    .await
    .expect("audit actions");
    assert_eq!(audit_actions, ["media.file.trashed", "media.file.uploaded"]);
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let second_read = service
        .get(&mut transaction, &first_context, second.id)
        .await;
    assert!(second_read.is_ok());
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let (_, public_bytes) = service
        .read_public_bytes(&mut transaction, &first_context, &store, second.id)
        .await
        .expect("public bytes");
    assert_eq!(public_bytes, PNG);
    let private_public_read = service
        .read_public_bytes(&mut transaction, &first_context, &store, first.id)
        .await;
    assert!(matches!(
        private_public_read,
        Err(MaviError::NotFound { .. })
    ));
    transaction.commit().await.expect("commit");
}
