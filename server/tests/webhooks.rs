//! A receiver that really answers, on a real socket, because what is being
//! asked is whether what left this process is what a receiver would accept.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use mavi::kernel::events::{self, EmitsEvents};
use mavi::kernel::http::AppState;
use mavi::kernel::webhook;
use uuid::Uuid;

mod common;

use common::harness;

struct Order(Uuid);

impl EmitsEvents for Order {
    const EVENTS: &'static [&'static str] = &["order.paid"];

    fn subject_id(&self) -> String {
        self.0.to_string()
    }

    fn payload(&self) -> serde_json::Value {
        serde_json::json!({ "total": 1234 })
    }
}

#[derive(Clone)]
struct Receiver {
    seen: Arc<AtomicUsize>,
    answer: StatusCode,
    signatures: Arc<std::sync::Mutex<Vec<(String, String, String)>>>,
}

async fn receive(State(receiver): State<Receiver>, headers: HeaderMap, body: String) -> StatusCode {
    receiver.seen.fetch_add(1, Ordering::SeqCst);

    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    };

    receiver.signatures.lock().expect("a lock").push((
        header("webhook-id"),
        header("webhook-timestamp"),
        format!("{}|{body}", header("webhook-signature")),
    ));

    receiver.answer
}

async fn a_receiver(answer: StatusCode) -> (String, Receiver) {
    let receiver = Receiver {
        seen: Arc::new(AtomicUsize::new(0)),
        answer,
        signatures: Arc::new(std::sync::Mutex::new(Vec::new())),
    };

    let app = Router::new()
        .route("/hook", post(receive))
        .with_state(receiver.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a socket");
    let address = listener.local_addr().expect("an address");

    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (format!("http://{address}/hook"), receiver)
}

/// The secret a receiver would be given, in the form the specification writes
/// it. Invented, and only this test ever sees it.
const SECRET: &str = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";

#[tokio::test]
async fn an_event_reaches_a_receiver_signed_the_way_the_specification_says() {
    let db = harness().await;

    let (url, receiver) = a_receiver(StatusCode::OK).await;

    let mut state = AppState::new(db.clone());
    state.allow_private_destinations = true;

    let mut conn = db.begin().await.expect("begin");

    sqlx::query(
        "insert into webhook_endpoints (url, secret, events)
         values ($1, $2, array['order.paid'])",
    )
    .bind(&url)
    .bind(SECRET)
    .execute(conn.conn())
    .await
    .expect("endpoint");

    let outbox_id = events::emit(&mut conn, "order.paid", &Order(Uuid::now_v7()))
        .await
        .expect("emit");

    conn.commit().await.expect("commit");

    // An event queues more than one thing — the fan-out to receivers, and
    // whatever the site arranged to happen next — so this works until there is
    // nothing left rather than counting ticks.
    for _ in 0..8 {
        mavi::jobs::tick(&state, "test").await.expect("tick");
    }

    assert_eq!(receiver.seen.load(Ordering::SeqCst), 1);

    let (id, timestamp, signed) = receiver.signatures.lock().expect("a lock")[0].clone();
    let (signature, body) = signed.split_once('|').expect("a signature and a body");

    assert_eq!(id, outbox_id.to_string());
    assert_eq!(
        signature,
        webhook::sign(
            &webhook::parse_secret(SECRET).expect("a secret"),
            &id,
            timestamp.parse().expect("a timestamp"),
            body
        ),
        "a receiver checking the signature would have refused this"
    );

    let mut conn = db.begin().await.expect("begin");
    let state_of: (String,) = sqlx::query_as("select state::text from outbox where id = $1")
        .bind(outbox_id)
        .fetch_one(conn.conn())
        .await
        .expect("outbox");

    assert_eq!(state_of.0, "delivered");
}

#[tokio::test]
async fn a_receiver_that_keeps_failing_ends_in_the_dead_letter() {
    let db = harness().await;

    let (url, receiver) = a_receiver(StatusCode::INTERNAL_SERVER_ERROR).await;

    let mut state = AppState::new(db.clone());
    state.allow_private_destinations = true;

    let mut conn = db.begin().await.expect("begin");

    sqlx::query(
        "insert into webhook_endpoints (url, secret, events)
         values ($1, $2, array['order.paid'])",
    )
    .bind(&url)
    .bind(SECRET)
    .execute(conn.conn())
    .await
    .expect("endpoint");

    let outbox_id = events::emit(&mut conn, "order.paid", &Order(Uuid::now_v7()))
        .await
        .expect("emit");

    conn.commit().await.expect("commit");

    // The queue is one table for every kind of work, so this drives ticks until this
    // job in particular has been given up on rather than counting them.
    let mut dead = false;

    for _ in 0..40 {
        let mut conn = db.begin().await.expect("begin");

        sqlx::query("update jobs set run_at = now() where kind = 'webhook.deliver'")
            .execute(conn.conn())
            .await
            .expect("walk forward");

        let state_of: Option<(String,)> = sqlx::query_as(
            "select j.state::text
               from jobs j
              where j.kind = 'webhook.deliver'
                and j.payload ->> 'outbox_id' = $1",
        )
        .bind(outbox_id.to_string())
        .fetch_optional(conn.conn())
        .await
        .expect("job");

        conn.commit().await.expect("commit");

        if state_of.is_some_and(|(state,)| state == "dead") {
            dead = true;
            break;
        }

        mavi::jobs::tick(&state, "test").await.expect("tick");
    }

    assert!(dead, "a receiver that never answers was retried forever");

    let mut conn = db.begin().await.expect("begin");

    let attempts: Vec<(i32, Option<i32>)> = sqlx::query_as(
        "select attempt, status_code from webhook_deliveries where outbox_id = $1 order by attempt",
    )
    .bind(outbox_id)
    .fetch_all(conn.conn())
    .await
    .expect("deliveries");

    assert_eq!(attempts.len(), 5, "every attempt is written down");
    assert!(attempts.iter().all(|(_, status)| *status == Some(500)));
    assert_eq!(receiver.seen.load(Ordering::SeqCst), 5);
}
