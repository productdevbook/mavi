//! What happened, said once and heard by whatever cares.
//!
//! A domain announces that something happened, and whatever this installation
//! arranged to follow one is queued beside it. Written into the outbox in the
//! same transaction as the change itself, so an event exists exactly when the
//! thing it is about did.
use sqlx::Row;
use uuid::Uuid;

use super::db::Tx;
use super::error::Result;
use super::http::AppState;

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

/// Writes the event down, and queues whatever this installation arranged to
/// follow one. What follows is handed in: the kernel is what announces, and
/// what listens is a domain's.
pub async fn emit<T: EmitsEvents>(
    state: &AppState,
    tx: &mut Tx,
    event: &str,
    subject: &T,
) -> Result<Uuid> {
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

    // Which name a graph reads this under is a thing the kernel is told, the
    // same way an endpoint is told which domain it belongs to.
    let counted_as = state
        .wiring
        .names_an_event
        .map_or("unnamed", |names_an_event| names_an_event(event));

    super::metrics::domain_emitted(counted_as, event);

    // Queued here rather than by a sweep over the table: the work belongs to
    // the transaction that made the change, and a row written and swept later
    // is a row that is sent twice when the change rolls back. All of it in this
    // transaction, so nothing is arranged for something that then rolled back.
    for follows in state.wiring.follows_an_event.iter().copied() {
        follows(tx, outbox_id).await?;
    }

    Ok(outbox_id)
}
