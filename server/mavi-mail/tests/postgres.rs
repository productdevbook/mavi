use std::env;

use chrono::{Duration, Utc};
use mavi_core::{
    MaviError, PageRequest, SiteContext, SiteId,
    ports::{MailContentType as CoreMailContentType, MailDeliveryReceipt, MailMessage, Seals},
};
use mavi_mail::{
    AddReader, CreateMailList, CreateMailTemplate, DeliveryListFilter, EnqueueDelivery,
    MailBounceClass, MailContentType, MailDeliveryStatus, MailProviderEventKind, MailService,
    MailTemplatePreview, ReaderListFilter, ReceiveMailProviderEvent, SendCampaign, UpdateMailList,
};
use mavi_sealing::KeyringSealer;
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
    let sealer = KeyringSealer::from_key([19; 32]);

    let mut transaction = database.begin(&first_context).await.expect("first scope");
    sqlx::query(
        "insert into site_settings
            (site_id, name, canonical_url, mail_sender_address, mail_sender_name)
         values ($1, $2, $3, $4, $5)",
    )
    .bind(first_site.into_uuid())
    .bind("Mail test")
    .bind("https://mail.example.test")
    .bind("noreply@example.test")
    .bind("Mail test")
    .execute(transaction.conn())
    .await
    .expect("settings");
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
    let sender = first_delivery.sender.as_ref().expect("delivery sender");
    assert_eq!(sender.address.as_str(), "noreply@example.test");
    assert_eq!(sender.name.as_deref(), Some("Mail test"));
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
            &sealer,
        )
        .await
        .expect("campaign");
    assert_eq!(campaign.enqueued, 1);
    let (link_ciphertext, token_hash): (Vec<u8>, Vec<u8>) = sqlx::query_as(
        "select l.ciphertext, t.token_hash
           from mail_delivery_links l
           join mail_unsubscribe_tokens t
             on t.site_id = l.site_id and t.delivery_id = l.delivery_id
          where l.site_id = $1
          limit 1",
    )
    .bind(first_site.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("campaign unsubscribe link");
    let unsubscribe_url = String::from_utf8(
        sealer
            .unseal(&first_context, &link_ciphertext)
            .await
            .expect("unseal campaign link"),
    )
    .expect("campaign URL");
    assert!(unsubscribe_url.starts_with("https://mail.example.test/public/v1/mail/unsubscribe/"));
    assert_ne!(link_ciphertext, unsubscribe_url.as_bytes());
    assert_eq!(token_hash.len(), 32);
    let campaign_token = unsubscribe_url
        .rsplit('/')
        .next()
        .expect("campaign token")
        .to_owned();
    service
        .unsubscribe(&mut transaction, &first_context, &campaign_token)
        .await
        .expect("campaign unsubscribe");
    service
        .unsubscribe(&mut transaction, &first_context, &campaign_token)
        .await
        .expect("campaign unsubscribe is idempotent");
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

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn protected_transactional_mail_is_sealed_redacted_and_unsealed_by_workers() {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2).await.expect("database");
    database.migrate().await.expect("migrations");

    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");
    let context = SiteContext::public(site_id);
    let service = MailService;
    let sealer = KeyringSealer::from_key([7; 32]);
    let secret_body = "Use this one-time token: mavi_reset_test-secret";

    let mut protected_transaction = database.begin(&context).await.expect("enqueue scope");
    let protected = service
        .enqueue_protected_transactional_message(
            &mut protected_transaction,
            &context,
            MailMessage {
                recipient: "security@example.test".to_owned(),
                subject: "Security message".to_owned(),
                body: secret_body.to_owned(),
                content_type: CoreMailContentType::Plain,
                unsubscribe_url: None,
            },
            Some("protected-test-1"),
            &sealer,
        )
        .await
        .expect("protected delivery");
    let plain = service
        .enqueue_transactional_message(
            &mut protected_transaction,
            &context,
            MailMessage {
                recipient: "plain@example.test".to_owned(),
                subject: "Plain message".to_owned(),
                body: "This is an ordinary message".to_owned(),
                content_type: CoreMailContentType::Plain,
                unsubscribe_url: None,
            },
            Some("plain-test-1"),
        )
        .await
        .expect("plain delivery");
    assert!(protected.body_protected);
    assert_eq!(protected.body, mavi_mail::PROTECTED_BODY_REDACTION);
    let (stored_body, stored_protected, ciphertext): (String, bool, Vec<u8>) = sqlx::query_as(
        "select d.body, d.body_protected, s.ciphertext
           from mail_deliveries d
           join mail_delivery_secrets s on s.site_id = d.site_id and s.delivery_id = d.id
          where d.site_id = $1 and d.id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(protected.id.into_uuid())
    .fetch_one(protected_transaction.conn())
    .await
    .expect("protected storage");
    assert_eq!(stored_body, mavi_mail::PROTECTED_BODY_REDACTION);
    assert!(stored_protected);
    assert!(
        !ciphertext
            .windows(secret_body.len())
            .any(|window| window == secret_body.as_bytes())
    );
    protected_transaction
        .commit()
        .await
        .expect("enqueue commit");

    let mut export_transaction = database.begin(&context).await.expect("export scope");
    let relocation = service
        .export_for_relocation(&mut export_transaction, &context)
        .await
        .expect("protected export");
    let relocated = relocation
        .deliveries
        .iter()
        .find(|delivery| delivery.id == protected.id.into_uuid())
        .expect("relocated protected delivery");
    assert_eq!(relocated.body, mavi_mail::PROTECTED_BODY_REDACTION);
    assert!(relocated.body_protected);
    assert_eq!(relocated.status, "cancelled");
    export_transaction.commit().await.expect("export commit");

    let mut no_key_transaction = database.begin(&context).await.expect("no-key scope");
    let plain_claimed = service
        .claim_next(
            &mut no_key_transaction,
            &context,
            "mail-worker-no-key",
            Utc::now() + Duration::minutes(5),
        )
        .await
        .expect("plain claim without sealer")
        .expect("plain delivery remains claimable");
    assert_eq!(plain_claimed.delivery.id, plain.id);
    assert_eq!(plain_claimed.message.body, "This is an ordinary message");
    service
        .mark_sent(
            &mut no_key_transaction,
            &context,
            plain.id,
            "mail-worker-no-key",
            &MailDeliveryReceipt {
                provider: "test-provider".to_owned(),
                reference: "plain-1".to_owned(),
            },
        )
        .await
        .expect("plain delivery sent");
    no_key_transaction
        .commit()
        .await
        .expect("plain claim commit");

    let mut protected_claim_transaction = database.begin(&context).await.expect("claim scope");
    let claimed = service
        .claim_next_with_sealer(
            &mut protected_claim_transaction,
            &context,
            "mail-worker-protected",
            Utc::now() + Duration::minutes(5),
            &sealer,
        )
        .await
        .expect("protected claim")
        .expect("protected delivery claim");
    assert_eq!(claimed.delivery.id, protected.id);
    assert!(claimed.delivery.body_protected);
    assert_eq!(claimed.delivery.body, mavi_mail::PROTECTED_BODY_REDACTION);
    assert_eq!(claimed.message.body, secret_body);
    protected_claim_transaction
        .commit()
        .await
        .expect("protected claim commit");

    let mut supplied_relocation = relocation.clone();
    supplied_relocation
        .deliveries
        .iter_mut()
        .find(|delivery| delivery.id == protected.id.into_uuid())
        .expect("supplied protected delivery")
        .status = "queued".to_owned();
    let mut import_transaction = database.begin(&context).await.expect("import scope");
    service
        .import_for_relocation(&mut import_transaction, &context, &supplied_relocation)
        .await
        .expect("protected import");
    import_transaction.commit().await.expect("import commit");

    let mut cleanup_transaction = database.begin(&context).await.expect("cleanup scope");
    let remaining_secrets: i64 =
        sqlx::query_scalar("select count(*) from mail_delivery_secrets where site_id = $1")
            .bind(site_id.into_uuid())
            .fetch_one(cleanup_transaction.conn())
            .await
            .expect("protected secret count");
    assert_eq!(remaining_secrets, 0);
    let imported = service
        .get_delivery(&mut cleanup_transaction, &context, protected.id)
        .await
        .expect("imported protected delivery");
    assert_eq!(imported.status, MailDeliveryStatus::Cancelled);
    assert!(matches!(
        service
            .retry_delivery(&mut cleanup_transaction, &context, protected.id)
            .await,
        Err(MaviError::Conflict { .. })
    ));
    cleanup_transaction.commit().await.expect("cleanup commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn provider_events_are_idempotent_and_suppress_future_campaigns() {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2).await.expect("database");
    database.migrate().await.expect("migrations");

    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");
    let context = SiteContext::public(site_id);
    let service = MailService;
    let sealer = KeyringSealer::from_key([23; 32]);

    let mut transaction = database.begin(&context).await.expect("setup scope");
    sqlx::query(
        "insert into site_settings (site_id, name, canonical_url)
         values ($1, $2, $3)",
    )
    .bind(site_id.into_uuid())
    .bind("Provider event test")
    .bind("https://provider-events.example.test")
    .execute(transaction.conn())
    .await
    .expect("settings");
    let list = service
        .create_list(
            &mut transaction,
            &context,
            &CreateMailList {
                slug: "deliverability".to_owned(),
                name: "Deliverability".to_owned(),
            },
        )
        .await
        .expect("list");
    let reader = service
        .add_reader(
            &mut transaction,
            &context,
            list.id,
            &AddReader {
                email: "bounce@example.test".to_owned(),
                name: None,
                resubscribe: false,
            },
        )
        .await
        .expect("reader");
    let template = service
        .create_template(
            &mut transaction,
            &context,
            &CreateMailTemplate {
                key: "deliverability".to_owned(),
                language: "en".to_owned(),
                subject: "A campaign".to_owned(),
                body: "A campaign body".to_owned(),
                content_type: MailContentType::Plain,
            },
        )
        .await
        .expect("template");
    let count = service
        .send_campaign(
            &mut transaction,
            &context,
            list.id,
            &SendCampaign {
                template_id: template.id,
                variables: Map::new(),
                idempotency_key: Some("provider-event-campaign".to_owned()),
            },
            &sealer,
        )
        .await
        .expect("campaign");
    assert_eq!(count.enqueued, 1);
    let delivery_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "select id from mail_deliveries where site_id = $1 and recipient = $2",
    )
    .bind(site_id.into_uuid())
    .bind("bounce@example.test")
    .fetch_one(transaction.conn())
    .await
    .expect("delivery")
    .into();

    let event = ReceiveMailProviderEvent {
        provider: "gateway".to_owned(),
        event_id: "gateway-event-1".to_owned(),
        delivery_id: Some(delivery_id),
        recipient: "bounce@example.test".to_owned(),
        kind: MailProviderEventKind::Bounced,
        bounce_class: Some(MailBounceClass::Permanent),
        provider_reference: Some("provider-message-1".to_owned()),
        reason: Some("mailbox does not exist".to_owned()),
        occurred_at: Utc::now(),
    };
    let receipt = service
        .receive_provider_event(&mut transaction, &context, &event)
        .await
        .expect("provider event");
    assert!(!receipt.duplicate);
    assert!(receipt.suppressed);
    assert_eq!(receipt.cancelled_deliveries, 1);

    let standing: String =
        sqlx::query_scalar("select standing from mail_readers where site_id = $1 and id = $2")
            .bind(site_id.into_uuid())
            .bind(reader.reader.id.into_uuid())
            .fetch_one(transaction.conn())
            .await
            .expect("standing");
    assert_eq!(standing, "bounced");
    let status: String =
        sqlx::query_scalar("select status from mail_deliveries where site_id = $1 and id = $2")
            .bind(site_id.into_uuid())
            .bind(delivery_id.into_uuid())
            .fetch_one(transaction.conn())
            .await
            .expect("cancelled delivery");
    assert_eq!(status, "cancelled");

    assert!(matches!(
        service
            .add_reader(
                &mut transaction,
                &context,
                list.id,
                &AddReader {
                    email: "bounce@example.test".to_owned(),
                    name: None,
                    resubscribe: true,
                },
            )
            .await,
        Err(MaviError::Conflict { code }) if code == "mail_reader_suppressed"
    ));

    let duplicate = service
        .receive_provider_event(&mut transaction, &context, &event)
        .await
        .expect("duplicate event");
    assert!(duplicate.duplicate);
    assert!(!duplicate.suppressed);
    assert_eq!(duplicate.cancelled_deliveries, 0);
    transaction.commit().await.expect("event commit");
}
