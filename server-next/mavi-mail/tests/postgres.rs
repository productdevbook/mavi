use std::env;

use chrono::{Duration, Utc};
use mavi_core::{MaviError, PageRequest, SiteContext, SiteId, ports::MailDeliveryReceipt};
use mavi_mail::{
    AddReader, CreateMailList, CreateMailTemplate, DeliveryListFilter, EnqueueDelivery,
    MailContentType, MailDeliveryStatus, MailService, MailTemplatePreview, ReaderListFilter,
    SendCampaign, UpdateMailList,
};
use mavi_storage::Database;
use serde_json::{Map, json};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn mail_templates_lists_and_outbox_are_site_scoped_and_leaseable() {
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
    let service = MailService;

    let mut transaction = database.begin(&first_context).await.expect("first scope");
    let template = service
        .create_template(
            &mut transaction,
            &first_context,
            &CreateMailTemplate {
                key: "welcome".to_owned(),
                language: "en".to_owned(),
                subject: "Hello {{name}}".to_owned(),
                body: "Welcome, {{name}}. You have {{count}} messages.".to_owned(),
                content_type: MailContentType::Plain,
            },
        )
        .await
        .expect("template");
    let rendered = service
        .preview_template(
            &mut transaction,
            &first_context,
            template.id,
            &MailTemplatePreview {
                variables: serde_json::from_value(json!({"name": "Ada", "count": 3}))
                    .expect("variables"),
            },
        )
        .await
        .expect("preview");
    assert_eq!(rendered.subject, "Hello Ada");
    assert_eq!(rendered.body, "Welcome, Ada. You have 3 messages.");

    let list = service
        .create_list(
            &mut transaction,
            &first_context,
            &CreateMailList {
                slug: "updates".to_owned(),
                name: "Product updates".to_owned(),
            },
        )
        .await
        .expect("list");
    let first_reader = service
        .add_reader(
            &mut transaction,
            &first_context,
            list.id,
            &AddReader {
                email: "ada@example.test".to_owned(),
                name: Some("Ada".to_owned()),
                resubscribe: false,
            },
        )
        .await
        .expect("first reader");
    let second_reader = service
        .add_reader(
            &mut transaction,
            &first_context,
            list.id,
            &AddReader {
                email: "grace@example.test".to_owned(),
                name: Some("Grace".to_owned()),
                resubscribe: false,
            },
        )
        .await
        .expect("second reader");
    assert_ne!(
        first_reader.unsubscribe_token,
        second_reader.unsubscribe_token
    );

    let readers = service
        .list_readers(
            &mut transaction,
            &first_context,
            list.id,
            &ReaderListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(1),
                },
                standing: None,
            },
        )
        .await
        .expect("reader page");
    assert_eq!(readers.items.len(), 1);
    assert!(readers.next_cursor.is_some());

    let unsubscribed = service
        .unsubscribe(
            &mut transaction,
            &first_context,
            &first_reader.unsubscribe_token,
        )
        .await
        .expect("unsubscribe");
    assert!(unsubscribed.unsubscribed);
    let subscribed = service
        .get_list(&mut transaction, &first_context, list.id)
        .await
        .expect("list after unsubscribe");
    assert_eq!(subscribed.subscriber_count, 1);

    let first_delivery = service
        .enqueue_delivery(
            &mut transaction,
            &first_context,
            &EnqueueDelivery {
                recipient: "grace@example.test".to_owned(),
                template_id: template.id,
                variables: serde_json::from_value(json!({"name": "Grace", "count": 1}))
                    .expect("variables"),
                idempotency_key: Some("welcome-grace-1".to_owned()),
            },
        )
        .await
        .expect("delivery");
    let duplicate = service
        .enqueue_delivery(
            &mut transaction,
            &first_context,
            &EnqueueDelivery {
                recipient: "grace@example.test".to_owned(),
                template_id: template.id,
                variables: Map::new(),
                idempotency_key: Some("welcome-grace-1".to_owned()),
            },
        )
        .await
        .expect("idempotent delivery");
    assert_eq!(duplicate.id, first_delivery.id);
    transaction.commit().await.expect("first commit");

    let mut transaction = database.begin(&first_context).await.expect("claim scope");
    let claimed = service
        .claim_next(
            &mut transaction,
            &first_context,
            "mail-worker-a",
            Utc::now() + Duration::minutes(5),
        )
        .await
        .expect("claim")
        .expect("a delivery");
    assert_eq!(claimed.delivery.id, first_delivery.id);
    assert_eq!(claimed.delivery.status, MailDeliveryStatus::Sending);
    assert_eq!(claimed.message.recipient, "grace@example.test");
    transaction.commit().await.expect("claim commit");

    let mut transaction = database.begin(&first_context).await.expect("sent scope");
    let sent = service
        .mark_sent(
            &mut transaction,
            &first_context,
            first_delivery.id,
            "mail-worker-a",
            &MailDeliveryReceipt {
                provider: "test-provider".to_owned(),
                reference: "provider-1".to_owned(),
            },
        )
        .await
        .expect("mark sent");
    assert_eq!(sent.status, MailDeliveryStatus::Sent);
    transaction.commit().await.expect("sent commit");

    let mut transaction = database
        .begin(&first_context)
        .await
        .expect("second enqueue scope");
    let failed_delivery = service
        .enqueue_delivery(
            &mut transaction,
            &first_context,
            &EnqueueDelivery {
                recipient: "grace@example.test".to_owned(),
                template_id: template.id,
                variables: serde_json::from_value(json!({"name": "Grace", "count": 2}))
                    .expect("variables"),
                idempotency_key: Some("welcome-grace-2".to_owned()),
            },
        )
        .await
        .expect("failed delivery");
    transaction.commit().await.expect("second enqueue commit");

    let mut transaction = database
        .begin(&first_context)
        .await
        .expect("failure claim scope");
    let claimed = service
        .claim_next(
            &mut transaction,
            &first_context,
            "mail-worker-b",
            Utc::now() + Duration::minutes(5),
        )
        .await
        .expect("claim failed delivery")
        .expect("failed delivery claim");
    assert_eq!(claimed.delivery.id, failed_delivery.id);
    transaction.commit().await.expect("failure claim commit");

    let mut transaction = database.begin(&first_context).await.expect("retry scope");
    let retry = service
        .mark_failed(
            &mut transaction,
            &first_context,
            failed_delivery.id,
            "mail-worker-b",
            "provider unavailable",
            Some(Utc::now() - Duration::seconds(1)),
        )
        .await
        .expect("mark retry");
    assert_eq!(retry.status, MailDeliveryStatus::Retry);
    transaction.commit().await.expect("retry commit");

    let mut transaction = database
        .begin(&first_context)
        .await
        .expect("dead claim scope");
    let claimed = service
        .claim_next(
            &mut transaction,
            &first_context,
            "mail-worker-c",
            Utc::now() + Duration::minutes(5),
        )
        .await
        .expect("claim retry")
        .expect("retry claim");
    assert_eq!(claimed.delivery.id, failed_delivery.id);
    transaction.commit().await.expect("dead claim commit");

    let mut transaction = database.begin(&first_context).await.expect("dead scope");
    let dead = service
        .mark_failed(
            &mut transaction,
            &first_context,
            failed_delivery.id,
            "mail-worker-c",
            "provider rejected message",
            None,
        )
        .await
        .expect("mark dead");
    assert_eq!(dead.status, MailDeliveryStatus::Dead);
    let requeued = service
        .retry_delivery(&mut transaction, &first_context, failed_delivery.id)
        .await
        .expect("manual retry");
    assert_eq!(requeued.status, MailDeliveryStatus::Queued);
    transaction.commit().await.expect("manual retry commit");

    let mut transaction = database
        .begin(&first_context)
        .await
        .expect("campaign scope");
    let campaign = service
        .send_campaign(
            &mut transaction,
            &first_context,
            list.id,
            &SendCampaign {
                template_id: template.id,
                variables: serde_json::from_value(json!({"name": "Reader", "count": 4}))
                    .expect("variables"),
                idempotency_key: Some("campaign-1".to_owned()),
            },
        )
        .await
        .expect("campaign");
    assert_eq!(campaign.enqueued, 1);
    let deliveries = service
        .list_deliveries(
            &mut transaction,
            &first_context,
            &DeliveryListFilter {
                page: PageRequest::default(),
                status: None,
            },
        )
        .await
        .expect("deliveries");
    assert!(deliveries.items.len() >= 3);
    let audit_count: i64 = sqlx::query_scalar(
        "select count(*) from audit_events where site_id = $1 and action like 'mail.%'",
    )
    .bind(first_site.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("mail audit count");
    assert!(audit_count >= 10);
    transaction.commit().await.expect("campaign commit");

    let second_context = SiteContext::public(second_site);
    let mut second_transaction = database.begin(&second_context).await.expect("second scope");
    let second_templates = service
        .list_templates(
            &mut second_transaction,
            &second_context,
            &mavi_mail::MailTemplateListFilter::default(),
        )
        .await
        .expect("second templates");
    assert!(second_templates.items.is_empty());
    assert!(matches!(
        service
            .get_delivery(&mut second_transaction, &second_context, first_delivery.id)
            .await,
        Err(MaviError::NotFound { .. })
    ));
    let _ = service
        .update_list(
            &mut second_transaction,
            &second_context,
            list.id,
            &UpdateMailList {
                name: Some("must not cross sites".to_owned()),
            },
        )
        .await
        .expect_err("cross-site list update");
    second_transaction.commit().await.expect("second commit");
}
