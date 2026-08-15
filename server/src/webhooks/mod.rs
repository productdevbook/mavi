//! Telling somebody else's software what happened here.
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::db::Tx;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::AppState;
use crate::kernel::queue::{self, Task};
use crate::kernel::webhook;

/// Long enough for a receiver that is thinking, short enough that a receiver
/// that has stopped answering does not hold a worker.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// One event that has been written and now has to reach whoever asked for it.
#[derive(Debug, Serialize, Deserialize)]
pub struct Dispatch {
    pub outbox_id: Uuid,
}

impl Task for Dispatch {
    const KIND: &'static str = "webhook.dispatch";
}

/// One event, one receiver. A receiver that is slow holds up its own deliveries
/// and nobody else's.
#[derive(Debug, Serialize, Deserialize)]
pub struct Deliver {
    pub outbox_id: Uuid,
    pub endpoint_id: Uuid,
}

impl Task for Deliver {
    const KIND: &'static str = "webhook.deliver";
}

#[must_use]
pub fn kinds() -> Vec<String> {
    vec![Dispatch::KIND.to_owned(), Deliver::KIND.to_owned()]
}

pub async fn dispatch(state: &AppState, task: &Dispatch) -> Result<()> {
    let mut conn = state.db.begin().await?;

    let event: (String,) = sqlx::query_as("select event from outbox where id = $1")
        .bind(task.outbox_id)
        .fetch_one(conn.conn())
        .await?;

    let endpoints: Vec<(Uuid,)> =
        sqlx::query_as("select id from webhook_endpoints where active and $1 = any (events)")
            .bind(&event.0)
            .fetch_all(conn.conn())
            .await?;

    for (endpoint_id,) in endpoints {
        queue::enqueue(
            &mut conn,
            &Deliver {
                outbox_id: task.outbox_id,
                endpoint_id,
            },
            None,
        )
        .await?;
    }

    sqlx::query("update outbox set state = 'delivering' where id = $1")
        .bind(task.outbox_id)
        .execute(conn.conn())
        .await?;

    conn.commit().await?;

    Ok(())
}

pub async fn deliver(state: &AppState, task: &Deliver, attempt: i32) -> Result<()> {
    let mut conn = state.db.begin().await?;

    let event: (String, serde_json::Value, Option<String>) =
        sqlx::query_as("select event, payload, subject_id from outbox where id = $1")
            .bind(task.outbox_id)
            .fetch_one(conn.conn())
            .await?;

    let endpoint: (String, String, i32) =
        sqlx::query_as("select url, secret, secret_version from webhook_endpoints where id = $1")
            .bind(task.endpoint_id)
            .fetch_one(conn.conn())
            .await?;

    let body = serde_json::json!({
        "id": task.outbox_id,
        "type": event.0,
        "subject": event.2,
        "data": event.1,
    })
    .to_string();

    let secret = webhook::parse_secret(&endpoint.1)?;
    let timestamp = state.clock.now().timestamp();
    let signature = webhook::sign(&secret, &task.outbox_id.to_string(), timestamp, &body);

    let outcome = send(
        state,
        &endpoint.0,
        &task.outbox_id.to_string(),
        timestamp,
        &signature,
        body,
    )
    .await;

    record(&mut conn, task, attempt, &outcome).await?;

    if let Sent::Answered { status, .. } = &outcome
        && (200..300).contains(status)
    {
        sqlx::query("update outbox set state = 'delivered', delivered_at = now() where id = $1")
            .bind(task.outbox_id)
            .execute(conn.conn())
            .await?;
    }

    conn.commit().await?;

    match outcome {
        Sent::Answered { status, .. } if (200..300).contains(&status) => Ok(()),
        Sent::Answered { status, .. } => Err(AppError::Bug(match status {
            400..=499 => "a receiver refused the event",
            _ => "a receiver failed on the event",
        })),
        Sent::Failed(_) => Err(AppError::Bug("a receiver could not be reached")),
    }
}

#[derive(Debug)]
enum Sent {
    Answered { status: i32, body: String },
    Failed(String),
}

async fn send(
    state: &AppState,
    url: &str,
    id: &str,
    timestamp: i64,
    signature: &str,
    body: String,
) -> Sent {
    match try_send(state, url, id, timestamp, signature, body).await {
        Ok(sent) => sent,
        Err(refusal) => Sent::Failed(refusal.to_string()),
    }
}

async fn try_send(
    state: &AppState,
    url: &str,
    id: &str,
    timestamp: i64,
    signature: &str,
    body: String,
) -> Result<Sent> {
    let sending =
        crate::kernel::outbound::reach(url, SEND_TIMEOUT, state.allow_private_destinations).await?;
    let (client, parsed) = (sending.client, sending.url);

    let response = client
        .post(parsed)
        .header("webhook-id", id)
        .header("webhook-timestamp", timestamp.to_string())
        .header("webhook-signature", signature)
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await;

    Ok(match response {
        Ok(response) => {
            let status = i32::from(response.status().as_u16());
            let body = response
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(2000)
                .collect();

            Sent::Answered { status, body }
        }
        Err(failure) => Sent::Failed(failure.without_url().to_string()),
    })
}

async fn record(conn: &mut Tx, task: &Deliver, attempt: i32, outcome: &Sent) -> Result<()> {
    let (status, response, failure) = match outcome {
        Sent::Answered { status, body } => (Some(*status), Some(body.clone()), None),
        Sent::Failed(why) => (None, None, Some(why.clone())),
    };

    sqlx::query(
        "insert into webhook_deliveries
             (endpoint_id, outbox_id, attempt, status_code, response, failure)
         values ($1, $2, $3, $4, $5, $6)",
    )
    .bind(task.endpoint_id)
    .bind(task.outbox_id)
    .bind(attempt)
    .bind(status)
    .bind(response)
    .bind(failure)
    .execute(conn.conn())
    .await?;

    Ok(())
}
