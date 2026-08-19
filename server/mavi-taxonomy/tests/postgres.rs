use std::env;

use mavi_content::{ContentService, CreateContent, PublicationInput};
use mavi_core::{MaviError, PageRequest, SiteContext, SiteId};
use mavi_storage::Database;
use mavi_taxonomy::{
    ContentTermAssignmentListFilter, CreateTerm, ReplaceContentTerms, TERM_CYCLE,
    TERM_PARENT_INVALID, TERM_PARENT_LANGUAGE_INVALID, TaxonomyService, TermKind, TermListFilter,
    UpdateTerm,
};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a PostgreSQL role that is subject to RLS"]
#[allow(clippy::too_many_lines)]
async fn taxonomy_terms_trees_assignments_and_filters_are_site_scoped() {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");

    let first = SiteId::new();
    let second = SiteId::new();
    database.ensure_site(first).await.expect("first site");
    database.ensure_site(second).await.expect("second site");

    let service = TaxonomyService;
    let context = SiteContext::public(first);
    let mut tx = database.begin(&context).await.expect("first scope");
    let parent = service
        .create_term(
            &mut tx,
            &context,
            &CreateTerm {
                kind: TermKind::Category,
                language: "en".to_owned(),
                slug: "news".to_owned(),
                name: "News".to_owned(),
                parent_id: None,
            },
        )
        .await
        .expect("parent term");
    let child = service
        .create_term(
            &mut tx,
            &context,
            &CreateTerm {
                kind: TermKind::Category,
                language: "en".to_owned(),
                slug: "local".to_owned(),
                name: "Local".to_owned(),
                parent_id: Some(parent.id),
            },
        )
        .await
        .expect("child term");
    let tag = service
        .create_term(
            &mut tx,
            &context,
            &CreateTerm {
                kind: TermKind::Tag,
                language: "en".to_owned(),
                slug: "featured".to_owned(),
                name: "Featured".to_owned(),
                parent_id: None,
            },
        )
        .await
        .expect("tag term");

    let tag_parent = service
        .create_term(
            &mut tx,
            &context,
            &CreateTerm {
                kind: TermKind::Tag,
                language: "en".to_owned(),
                slug: "invalid-parent".to_owned(),
                name: "Invalid parent".to_owned(),
                parent_id: Some(parent.id),
            },
        )
        .await
        .expect_err("tags cannot have parents");
    assert!(matches!(
        tag_parent,
        MaviError::Validation { code, .. } if code == TERM_PARENT_INVALID
    ));

    let different_language = service
        .create_term(
            &mut tx,
            &context,
            &CreateTerm {
                kind: TermKind::Category,
                language: "de".to_owned(),
                slug: "deutsch".to_owned(),
                name: "Deutsch".to_owned(),
                parent_id: Some(parent.id),
            },
        )
        .await
        .expect_err("parent language");
    assert!(matches!(
        different_language,
        MaviError::Validation { code, .. } if code == TERM_PARENT_LANGUAGE_INVALID
    ));

    let cycle = service
        .update_term(
            &mut tx,
            &context,
            parent.id,
            &UpdateTerm {
                name: None,
                parent_id: Some(Some(child.id)),
            },
        )
        .await
        .expect_err("category cycle");
    assert!(matches!(
        cycle,
        MaviError::Validation { code, .. } if code == TERM_CYCLE
    ));

    let roots = service
        .list_terms(
            &mut tx,
            &context,
            &TermListFilter {
                roots: true,
                kind: Some(TermKind::Category),
                ..TermListFilter::default()
            },
        )
        .await
        .expect("root categories");
    assert_eq!(roots.items.len(), 1);
    assert_eq!(roots.items[0].id, parent.id);

    let created_content = ContentService
        .create(
            &mut tx,
            &context,
            &CreateContent {
                kind: "post".to_owned(),
                language: "en".to_owned(),
                slug: "taxonomy-post".to_owned(),
                title: "Taxonomy post".to_owned(),
                excerpt: None,
                body: "Body".to_owned(),
                fields: serde_json::json!({}),
                publication: PublicationInput::Draft,
            },
            chrono::Utc::now(),
        )
        .await
        .expect("content");

    let assigned = service
        .replace_content_terms(
            &mut tx,
            &context,
            created_content.id,
            &ReplaceContentTerms {
                term_ids: vec![parent.id, tag.id, tag.id],
            },
        )
        .await
        .expect("replace assignments");
    assert_eq!(assigned.len(), 2);

    let content_terms = service
        .list_content_terms(&mut tx, &context, created_content.id)
        .await
        .expect("content terms");
    assert_eq!(content_terms.len(), 2);

    let assignments = service
        .list_term_content(
            &mut tx,
            &context,
            parent.id,
            &ContentTermAssignmentListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(1),
                },
            },
        )
        .await
        .expect("term content");
    assert_eq!(assignments.items.len(), 1);
    assert_eq!(assignments.items[0].content_id, created_content.id);
    assert!(assignments.next_cursor.is_none());

    service
        .delete_term(&mut tx, &context, parent.id)
        .await
        .expect("delete parent");
    let child_after_delete = service
        .get_term(&mut tx, &context, child.id)
        .await
        .expect("child remains");
    assert!(child_after_delete.parent_id.is_none());
    let terms_after_delete = service
        .list_content_terms(&mut tx, &context, created_content.id)
        .await
        .expect("remaining content terms");
    assert_eq!(terms_after_delete.len(), 1);
    assert_eq!(terms_after_delete[0].id, tag.id);

    let audit_count: i64 = sqlx::query_scalar(
        "select count(*) from audit_events where site_id = $1 and action like 'taxonomy.%'",
    )
    .bind(first.into_uuid())
    .fetch_one(tx.conn())
    .await
    .expect("taxonomy audit count");
    assert_eq!(audit_count, 5);
    tx.commit().await.expect("first commit");

    let second_context = SiteContext::public(second);
    let mut second_tx = database.begin(&second_context).await.expect("second scope");
    let second_terms = service
        .list_terms(&mut second_tx, &second_context, &TermListFilter::default())
        .await
        .expect("second site terms");
    assert!(second_terms.items.is_empty());
    assert!(matches!(
        service
            .list_term_content(
                &mut second_tx,
                &second_context,
                parent.id,
                &ContentTermAssignmentListFilter::default(),
            )
            .await,
        Err(MaviError::NotFound { .. })
    ));
    second_tx.commit().await.expect("second commit");
}
