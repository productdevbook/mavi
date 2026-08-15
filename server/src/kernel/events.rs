//! What happened, said once and heard by whatever cares.
//!
//! A domain announces — a post published, an order paid — and flows and
//! webhooks are what listen. Written into the outbox in the same transaction
//! as the change itself, so an event exists exactly when the thing it is about
//! did.
use sqlx::Row;
use uuid::Uuid;

use super::db::Tx;
use super::error::Result;
use super::queue;

/// What a domain change tells the outside world.
///
/// The row goes in the same transaction as the change it describes, so nothing
/// is ever announced that then rolled back, and nothing commits silently that
/// should have been announced. Delivery is a separate reader over `outbox`,
/// signing what it sends the way Standard Webhooks says to.
pub trait EmitsEvents {
    /// The event names this type can publish under — `order.paid`,
    /// `post.published`. Listed so the panel can offer them and a subscription
    /// to a name nothing emits can be refused.
    const EVENTS: &'static [&'static str];

    fn subject_id(&self) -> String;

    fn payload(&self) -> serde_json::Value;
}

pub async fn emit<T: EmitsEvents>(tx: &mut Tx, event: &str, subject: &T) -> Result<Uuid> {
    debug_assert!(
        T::EVENTS.contains(&event),
        "{event} is not one of the events this type declares"
    );

    let row = sqlx::query(
        "insert into outbox (event, subject_id, payload)
         values ($1, $2, $3)
         returning id",
    )
    .bind(event)
    .bind(subject.subject_id())
    .bind(subject.payload())
    .fetch_one(tx.conn())
    .await?;

    let outbox_id: Uuid = row.get("id");

    super::metrics::domain_emitted(super::domain::of_event(event), event);

    // Enqueued here rather than by a sweep over the table: the work belongs to
    // the transaction that made the change, and a row written and swept later
    // is a row that is sent twice when the change rolls back.
    queue::enqueue(tx, &crate::webhooks::Dispatch { outbox_id }, None).await?;

    // And whatever the site arranged to happen when this occurs. Both queued
    // in this transaction, so neither is scheduled for something that then
    // rolled back.
    queue::enqueue(tx, &crate::flows::Start { outbox_id }, None).await?;

    Ok(outbox_id)
}
