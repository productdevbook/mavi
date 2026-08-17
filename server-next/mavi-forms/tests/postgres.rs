use std::env;

use mavi_core::{MaviError, PageRequest, SiteContext, SiteId};
use mavi_forms::{
    CreateForm, FormField, FormFieldKind, FormListFilter, FormService, SubmissionListFilter,
    SubmitForm,
};
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
