use std::env;

use mavi_content::{
    CONTENT_FIELD_REQUIRED, CONTENT_NOT_FOUND, ContentRevisionListFilter, ContentService,
    ContentTypeField, ContentTypeListFilter, CreateContent, DeclareContentType, FieldKind,
    PublicationInput, UpdateContent,
};
use mavi_core::{MaviError, PageRequest, SiteContext, SiteId};
use mavi_storage::Database;
use serde_json::json;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a PostgreSQL role that is subject to RLS"]
#[allow(clippy::too_many_lines)]
async fn content_types_are_site_scoped_and_validate_content_fields() {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");

    let first = SiteId::new();
    let second = SiteId::new();
    database.ensure_site(first).await.expect("first site");
    database.ensure_site(second).await.expect("second site");

    let service = ContentService;
    let first_context = SiteContext::public(first);
    let mut tx = database.begin(&first_context).await.expect("first scope");
    service
        .initialize(&mut tx, &first_context)
        .await
        .expect("default content types");

    let first_page = service
        .list_content_types(
            &mut tx,
            &first_context,
            &ContentTypeListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(1),
                },
            },
        )
        .await
        .expect("first content type page");
    assert_eq!(first_page.items.len(), 1);
    let cursor = first_page.next_cursor.clone().expect("content type cursor");

    let second_page = service
        .list_content_types(
            &mut tx,
            &first_context,
            &ContentTypeListFilter {
                page: PageRequest {
                    after: Some(cursor),
                    limit: Some(1),
                },
            },
        )
        .await
        .expect("second content type page");
    assert_eq!(second_page.items.len(), 1);
    assert!(second_page.next_cursor.is_none());

    let recipe = service
        .upsert_content_type(
            &mut tx,
            &first_context,
            "recipe",
            &DeclareContentType {
                name: "Recipe".to_owned(),
                fields: vec![
                    ContentTypeField {
                        key: "summary".to_owned(),
                        label: "Summary".to_owned(),
                        required: true,
                        kind: FieldKind::Text,
                        options: Vec::new(),
                    },
                    ContentTypeField {
                        key: "status".to_owned(),
                        label: "Status".to_owned(),
                        required: false,
                        kind: FieldKind::Choice,
                        options: vec!["draft".to_owned(), "ready".to_owned()],
                    },
                ],
            },
        )
        .await
        .expect("recipe type");
    assert_eq!(recipe.kind.as_str(), "recipe");

    let missing_required = service
        .create(
            &mut tx,
            &first_context,
            &CreateContent {
                kind: "recipe".to_owned(),
                language: "en".to_owned(),
                slug: "missing-summary".to_owned(),
                title: "Missing summary".to_owned(),
                excerpt: None,
                body: String::new(),
                fields: json!({"status": "draft"}),
                publication: PublicationInput::Draft,
            },
            chrono::Utc::now(),
        )
        .await
        .expect_err("required field");
    assert!(matches!(
        missing_required,
        MaviError::Validation { code, field: Some(field) }
            if code == CONTENT_FIELD_REQUIRED && field == "summary"
    ));

    let created = service
        .create(
            &mut tx,
            &first_context,
            &CreateContent {
                kind: "recipe".to_owned(),
                language: "en".to_owned(),
                slug: "valid-recipe".to_owned(),
                title: "Valid recipe".to_owned(),
                excerpt: None,
                body: "Mix it.".to_owned(),
                fields: json!({"summary": "A valid recipe", "status": "ready"}),
                publication: PublicationInput::Draft,
            },
            chrono::Utc::now(),
        )
        .await
        .expect("valid content");

    service
        .delete_content_type(&mut tx, &first_context, "recipe")
        .await
        .expect("delete declaration");
    let retained = service
        .get(&mut tx, &first_context, created.id)
        .await
        .expect("content remains after type deletion");
    assert_eq!(retained.slug.as_str(), "valid-recipe");

    let audited: i64 = sqlx::query_scalar(
        "select count(*) from audit_events where site_id = $1 and action in ('content.type.upserted', 'content.type.deleted')",
    )
    .bind(first.into_uuid())
    .fetch_one(tx.conn())
    .await
    .expect("content type audit count");
    assert_eq!(audited, 2);
    tx.commit().await.expect("first commit");

    let second_context = SiteContext::public(second);
    let mut second_tx = database.begin(&second_context).await.expect("second scope");
    let second_types = service
        .list_content_types(
            &mut second_tx,
            &second_context,
            &ContentTypeListFilter::default(),
        )
        .await
        .expect("second site types");
    assert!(second_types.items.is_empty());
    second_tx.commit().await.expect("second commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a PostgreSQL role that is subject to RLS"]
#[allow(clippy::too_many_lines)]
async fn content_revisions_and_slug_history_are_site_scoped() {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");

    let first = SiteId::new();
    let second = SiteId::new();
    database.ensure_site(first).await.expect("first site");
    database.ensure_site(second).await.expect("second site");

    let service = ContentService;
    let first_context = SiteContext::public(first);
    let mut tx = database.begin(&first_context).await.expect("first scope");
    let created = service
        .create(
            &mut tx,
            &first_context,
            &CreateContent {
                kind: "post".to_owned(),
                language: "en".to_owned(),
                slug: "old-path".to_owned(),
                title: "First title".to_owned(),
                excerpt: None,
                body: "First body".to_owned(),
                fields: json!({}),
                publication: PublicationInput::Draft,
            },
            chrono::Utc::now(),
        )
        .await
        .expect("create content");

    let updated = service
        .update(
            &mut tx,
            &first_context,
            created.id,
            &UpdateContent {
                slug: Some("new-path".to_owned()),
                title: Some("Updated title".to_owned()),
                ..UpdateContent::default()
            },
            chrono::Utc::now(),
        )
        .await
        .expect("update content");
    assert_eq!(updated.revision, 2);

    let published = service
        .publish(&mut tx, &first_context, created.id, chrono::Utc::now())
        .await
        .expect("publish content");
    assert_eq!(published.revision, 3);

    let first_page = service
        .list_revisions(
            &mut tx,
            &first_context,
            created.id,
            &ContentRevisionListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(1),
                },
            },
        )
        .await
        .expect("first revision page");
    assert_eq!(first_page.items.len(), 1);
    assert_eq!(first_page.items[0].revision, 3);
    let first_cursor = first_page.next_cursor.clone().expect("first cursor");

    let second_page = service
        .list_revisions(
            &mut tx,
            &first_context,
            created.id,
            &ContentRevisionListFilter {
                page: PageRequest {
                    after: Some(first_cursor),
                    limit: Some(1),
                },
            },
        )
        .await
        .expect("second revision page");
    assert_eq!(second_page.items.len(), 1);
    assert_eq!(second_page.items[0].revision, 2);
    let second_cursor = second_page.next_cursor.clone().expect("second cursor");

    let third_page = service
        .list_revisions(
            &mut tx,
            &first_context,
            created.id,
            &ContentRevisionListFilter {
                page: PageRequest {
                    after: Some(second_cursor),
                    limit: Some(1),
                },
            },
        )
        .await
        .expect("third revision page");
    assert_eq!(third_page.items.len(), 1);
    assert_eq!(third_page.items[0].revision, 1);
    assert!(third_page.next_cursor.is_none());

    let revision = service
        .read_revision(&mut tx, &first_context, created.id, 1)
        .await
        .expect("read first revision");
    assert_eq!(revision.slug.as_str(), "old-path");
    assert_eq!(revision.title, "First title");

    let public_old_path = service
        .public_get(&mut tx, &first_context, "en", "old-path")
        .await
        .expect("old published path");
    assert_eq!(public_old_path.id, created.id);
    assert_eq!(public_old_path.slug.as_str(), "new-path");

    let history_count: i64 = sqlx::query_scalar(
        "select count(*) from content_slug_history where site_id = $1 and content_id = $2",
    )
    .bind(first.into_uuid())
    .bind(created.id.into_uuid())
    .fetch_one(tx.conn())
    .await
    .expect("slug history count");
    assert_eq!(history_count, 1);

    let audit_count: i64 = sqlx::query_scalar(
        "select count(*) from audit_events where site_id = $1 and resource_id = $2 and action like 'content.%'",
    )
    .bind(first.into_uuid())
    .bind(created.id.into_uuid())
    .fetch_one(tx.conn())
    .await
    .expect("content audit count");
    assert_eq!(audit_count, 3);
    tx.commit().await.expect("first commit");

    let second_context = SiteContext::public(second);
    let mut second_tx = database.begin(&second_context).await.expect("second scope");
    assert!(matches!(
        service
            .get(&mut second_tx, &second_context, created.id)
            .await,
        Err(MaviError::NotFound {
            resource: CONTENT_NOT_FOUND
        })
    ));
    assert!(matches!(
        service
            .list_revisions(
                &mut second_tx,
                &second_context,
                created.id,
                &ContentRevisionListFilter::default(),
            )
            .await,
        Err(MaviError::NotFound {
            resource: CONTENT_NOT_FOUND
        })
    ));
    assert!(matches!(
        service
            .public_get(&mut second_tx, &second_context, "en", "old-path")
            .await,
        Err(MaviError::NotFound {
            resource: CONTENT_NOT_FOUND
        })
    ));
    second_tx.commit().await.expect("second commit");
}
