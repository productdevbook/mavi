use std::sync::{Arc, Mutex};

use chrono::Utc;
use mavi_core::{
    MaviError, Result, SiteContext, SiteId,
    ports::{BoxFuture, MailDeliveryReceipt, MailDeliveryRequest, Mailer, Seals},
};
use mavi_files::InMemoryFileStore;
use mavi_mail::MailService;
use mavi_sealing::KeyringSealer;
use mavi_storage::Database;
use mavi_worker::{WorkerConfig, WorkerSupervisor};

#[derive(Clone, Debug)]
struct RecordingMailer {
    calls: Arc<Mutex<Vec<(SiteId, MailDeliveryRequest)>>>,
    failure: Option<String>,
}

impl Mailer for RecordingMailer {
    fn send<'a>(
        &'a self,
        context: &'a SiteContext,
        request: MailDeliveryRequest,
    ) -> BoxFuture<'a, Result<MailDeliveryReceipt>> {
        let calls = Arc::clone(&self.calls);
        let failure = self.failure.clone();
        let site_id = context.site_id;
        Box::pin(async move {
            calls
                .lock()
                .expect("mailer calls lock")
                .push((site_id, request));
            if let Some(error) = failure {
                return Err(MaviError::conflict(error));
            }
            Ok(MailDeliveryReceipt {
                provider: "test-provider".to_owned(),
                reference: "test-reference".to_owned(),
            })
        })
    }
}

fn worker(database: Database, site_id: SiteId, mailer: Arc<dyn Mailer>) -> WorkerSupervisor {
    let sealer: Arc<dyn Seals> = Arc::new(KeyringSealer::from_key([31; 32]));
    WorkerSupervisor::new_with_mailer(
        database,
        [site_id],
        WorkerConfig::new("mail-test-worker", 30, std::time::Duration::from_millis(10))
            .expect("worker config"),
        Arc::new(InMemoryFileStore::default()),
        mailer,
        sealer,
    )
}

async fn enqueue_message(
    database: &Database,
    site_id: SiteId,
    key: &str,
) -> mavi_core::MailDeliveryId {
    let context = SiteContext::public(site_id);
    let mut transaction = database.begin(&context).await.expect("enqueue scope");
    let delivery = MailService
        .enqueue_transactional_message(
            &mut transaction,
            &context,
            mavi_core::ports::MailMessage {
                recipient: "ada@example.test".to_owned(),
                subject: "A queued message".to_owned(),
                body: "The worker should deliver this.".to_owned(),
                content_type: mavi_core::ports::MailContentType::Plain,
                unsubscribe_url: None,
            },
            Some(key),
        )
        .await
        .expect("delivery");
    transaction.commit().await.expect("enqueue commit");
    delivery.id
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn mail_worker_passes_stable_delivery_metadata_and_marks_sent() {
    let database_url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&database_url, 2).await.expect("database");
    database.migrate().await.expect("migrations");
    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");
    let delivery_id = enqueue_message(&database, site_id, "mail-worker-success-1").await;

    let calls = Arc::new(Mutex::new(Vec::new()));
    let mailer: Arc<dyn Mailer> = Arc::new(RecordingMailer {
        calls: Arc::clone(&calls),
        failure: None,
    });
    let supervisor = worker(database.clone(), site_id, mailer);

    assert!(supervisor.run_once(site_id).await.expect("worker run"));
    {
        let calls = calls.lock().expect("mailer calls lock");
        assert_eq!(calls.len(), 1);
        let (called_site, request) = &calls[0];
        assert_eq!(*called_site, site_id);
        assert_eq!(request.delivery_id.into_uuid(), delivery_id.into_uuid());
        assert_eq!(request.attempt_number, 1);
        assert_eq!(
            request.idempotency_key.as_deref(),
            Some("mail-worker-success-1")
        );
        assert_eq!(request.message.recipient, "ada@example.test");
    }

    let context = SiteContext::public(site_id);
    let mut transaction = database.begin(&context).await.expect("check scope");
    let (status, provider, reference, attempt_status): (
        String,
        Option<String>,
        Option<String>,
        String,
    ) = sqlx::query_as(
        "select d.status, d.provider, d.provider_reference, a.status
               from mail_deliveries d
               join mail_delivery_attempts a on a.site_id = d.site_id and a.delivery_id = d.id
              where d.site_id = $1 and d.id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(delivery_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("delivery state");
    assert_eq!(status, "sent");
    assert_eq!(provider.as_deref(), Some("test-provider"));
    assert_eq!(reference.as_deref(), Some("test-reference"));
    assert_eq!(attempt_status, "sent");
    transaction.commit().await.expect("check commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn mail_worker_records_provider_failures_as_retryable_without_leaking_controls() {
    let database_url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&database_url, 2).await.expect("database");
    database.migrate().await.expect("migrations");
    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");
    let delivery_id = enqueue_message(&database, site_id, "mail-worker-failure-1").await;

    let calls = Arc::new(Mutex::new(Vec::new()));
    let mailer: Arc<dyn Mailer> = Arc::new(RecordingMailer {
        calls,
        failure: Some("provider_temporarily_unavailable\n".to_owned()),
    });
    let supervisor = worker(database.clone(), site_id, mailer);

    assert!(supervisor.run_once(site_id).await.expect("worker run"));
    let context = SiteContext::public(site_id);
    let mut transaction = database.begin(&context).await.expect("check scope");
    let (status, attempts, last_error, available_at): (
        String,
        i16,
        Option<String>,
        chrono::DateTime<Utc>,
    ) = sqlx::query_as(
        "select status, attempts, last_error, available_at
               from mail_deliveries where site_id = $1 and id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(delivery_id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("failed delivery state");
    assert_eq!(status, "retry");
    assert_eq!(attempts, 1);
    let last_error = last_error.expect("provider error");
    assert!(last_error.contains("provider_temporarily_unavailable"));
    assert!(!last_error.contains('\n'));
    assert!(available_at > Utc::now());
    transaction.commit().await.expect("check commit");
}
