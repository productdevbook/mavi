use chrono::Utc;
use mavi_content::{ContentService, CreateContent, PublicationInput};
use mavi_core::ports::FileStore;
use mavi_core::{MaviError, PageRequest, SiteContext, SiteId};
use mavi_files::InMemoryFileStore;
use mavi_media::{FileVisibility, MediaService};
use mavi_storage::Database;
use mavi_taxonomy::{CreateTerm, TaxonomyService, TermKind};
use mavi_trash::{TrashKind, TrashListFilter, TrashService};
use serde_json::json;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn trash_lists_restores_and_permanently_deletes_site_resources() {
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

    let context = SiteContext::public(first_site);
    let content_service = ContentService;
    let taxonomy_service = TaxonomyService;
    let media_service = MediaService;
    let trash_service = TrashService;
    let store = InMemoryFileStore::default();

    let mut transaction = database.begin(&context).await.expect("transaction");
    content_service
        .initialize(&mut transaction, &context)
        .await
        .expect("content types");
    let content_entry = content_service
        .create(
            &mut transaction,
            &context,
            &CreateContent {
                kind: "post".to_owned(),
                language: "en".to_owned(),
                slug: "trash-content".to_owned(),
                title: "Trash content".to_owned(),
                excerpt: None,
                body: "body".to_owned(),
                fields: json!({}),
                publication: PublicationInput::Draft,
            },
            Utc::now(),
        )
        .await
        .expect("content");
    let term = taxonomy_service
        .create_term(
            &mut transaction,
            &context,
            &CreateTerm {
                kind: TermKind::Tag,
                language: "en".to_owned(),
                slug: "trash-term".to_owned(),
                name: "Trash term".to_owned(),
                parent_id: None,
            },
        )
        .await
        .expect("term");
    let file = media_service
        .upload(
            &mut transaction,
            &context,
            &store,
            "trash.png",
            FileVisibility::Private,
            PNG.to_vec(),
        )
        .await
        .expect("file");
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&context).await.expect("transaction");
    content_service
        .trash(&mut transaction, &context, content_entry.id)
        .await
        .expect("trash content");
    taxonomy_service
        .delete_term(&mut transaction, &context, term.id)
        .await
        .expect("trash term");
    media_service
        .trash(&mut transaction, &context, file.id)
        .await
        .expect("trash file");
    transaction.commit().await.expect("commit");

    let relocation_site = SiteId::new();
    database
        .ensure_site(relocation_site)
        .await
        .expect("relocation site");
    let mut transaction = database.begin(&context).await.expect("export transaction");
    let relocation = trash_service
        .export_for_relocation(&mut transaction, &context, &store)
        .await
        .expect("export trash relocation");
    assert_eq!(relocation.source_site_id, first_site);
    assert_eq!(relocation.content.len(), 1);
    assert!(!relocation.revisions.is_empty());
    assert_eq!(relocation.terms.len(), 1);
    assert_eq!(relocation.files.len(), 1);
    assert_eq!(relocation.files[0].id, file.id.into_uuid());
    transaction.commit().await.expect("export commit");

    let relocation_context = SiteContext::public(relocation_site);
    let mut relocation = relocation;
    relocation.source_site_id = relocation_site;
    let mut transaction = database
        .begin(&relocation_context)
        .await
        .expect("relocation transaction");
    trash_service
        .import_for_relocation(&mut transaction, &relocation_context, &store, &relocation)
        .await
        .expect("import trash relocation");
    let imported = trash_service
        .list(
            &mut transaction,
            &relocation_context,
            &TrashListFilter::default(),
        )
        .await
        .expect("imported trash list");
    assert_eq!(imported.items.len(), 3);
    trash_service
        .restore(
            &mut transaction,
            &relocation_context,
            TrashKind::Content,
            content_entry.id.into_uuid(),
        )
        .await
        .expect("restore imported content");
    trash_service
        .restore(
            &mut transaction,
            &relocation_context,
            TrashKind::Term,
            term.id.into_uuid(),
        )
        .await
        .expect("restore imported term");
    trash_service
        .restore(
            &mut transaction,
            &relocation_context,
            TrashKind::File,
            file.id.into_uuid(),
        )
        .await
        .expect("restore imported file");
    assert_eq!(
        store
            .get(&relocation_context, &relocation.files[0].storage_key)
            .await
            .expect("imported file bytes"),
        PNG
    );
    transaction.commit().await.expect("relocation commit");

    let mut transaction = database.begin(&context).await.expect("transaction");
    let first_page = trash_service
        .list(
            &mut transaction,
            &context,
            &TrashListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(2),
                },
                kind: None,
            },
        )
        .await
        .expect("first trash page");
    assert_eq!(first_page.items.len(), 2);
    let cursor = first_page.next_cursor.clone().expect("trash cursor");
    let second_page = trash_service
        .list(
            &mut transaction,
            &context,
            &TrashListFilter {
                page: PageRequest {
                    after: Some(cursor),
                    limit: Some(2),
                },
                kind: None,
            },
        )
        .await
        .expect("second trash page");
    assert_eq!(second_page.items.len(), 1);
    assert!(second_page.next_cursor.is_none());

    trash_service
        .restore(
            &mut transaction,
            &context,
            TrashKind::Content,
            content_entry.id.into_uuid(),
        )
        .await
        .expect("restore content");
    let restored = content_service
        .get(&mut transaction, &context, content_entry.id)
        .await
        .expect("restored content");
    assert_eq!(restored.id, content_entry.id);
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&context).await.expect("transaction");
    let file_deletion = trash_service
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::File,
            file.id.into_uuid(),
        )
        .await
        .expect("permanently delete file metadata");
    let storage_key = file_deletion
        .file_storage_key
        .clone()
        .expect("file storage key");
    assert!(store.get(&context, &storage_key).await.is_ok());
    transaction.commit().await.expect("commit");

    store
        .remove(&context, &storage_key)
        .await
        .expect("remove file bytes");
    let mut transaction = database.begin(&context).await.expect("transaction");
    media_service
        .complete_cleanup(
            &mut transaction,
            &context,
            mavi_core::FileId::from_uuid(file.id.into_uuid()),
        )
        .await
        .expect("complete cleanup");
    transaction.commit().await.expect("commit");
    assert!(store.get(&context, &storage_key).await.is_err());

    let mut transaction = database.begin(&context).await.expect("transaction");
    trash_service
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::Term,
            term.id.into_uuid(),
        )
        .await
        .expect("permanently delete term");
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&context).await.expect("transaction");
    content_service
        .trash(&mut transaction, &context, content_entry.id)
        .await
        .expect("trash restored content");
    trash_service
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::Content,
            content_entry.id.into_uuid(),
        )
        .await
        .expect("permanently delete content");
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&context).await.expect("transaction");
    let missing = content_service
        .get(&mut transaction, &context, content_entry.id)
        .await;
    assert!(matches!(missing, Err(MaviError::NotFound { .. })));
    let completed: bool = sqlx::query_scalar(
        "select completed_at is not null from media_cleanup_tasks where site_id = $1 and file_id = $2",
    )
    .bind(first_site.into_uuid())
    .bind(file.id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("cleanup receipt");
    assert!(completed);
    transaction.commit().await.expect("commit");

    let second_context = SiteContext::public(second_site);
    let mut transaction = database.begin(&second_context).await.expect("transaction");
    let isolated = trash_service
        .list(
            &mut transaction,
            &second_context,
            &TrashListFilter::default(),
        )
        .await
        .expect("isolated trash list");
    assert!(isolated.items.is_empty());
    transaction.commit().await.expect("commit");
}
