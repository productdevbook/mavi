//! Who did what.
//!
//! Every change writes a row before it can answer, and the router enforces it:
//! a handler that changes something and does not hand back a receipt does not
//! compile into a route. What changed is written as it was — before and after
//! — because a log that says only "changed" answers nothing anybody asks it.
use serde::Serialize;
use uuid::Uuid;

use super::db::TenantConn;
use super::error::Result;
use super::http::{Caller, RequestId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorKind {
    User,
    Student,
    System,
    Operator,
}

impl ActorKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ActorKind::User => "user",
            ActorKind::Student => "student",
            ActorKind::System => "system",
            ActorKind::Operator => "operator",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Actor {
    pub id: Option<Uuid>,
    pub kind: ActorKind,
    pub request_id: RequestId,
}

impl Actor {
    #[must_use]
    pub fn of(caller: &Caller) -> Self {
        Self {
            id: caller.user.as_ref().map(|user| user.user_id),
            kind: match caller.user {
                Some(_) => ActorKind::User,
                None => ActorKind::System,
            },
            request_id: caller.request_id,
        }
    }

    #[must_use]
    pub fn system(request_id: RequestId) -> Self {
        Self {
            id: None,
            kind: ActorKind::System,
            request_id,
        }
    }
}

/// What a change records about itself. A domain type implements this instead of
/// each handler remembering to write a row.
pub trait Auditable {
    const SUBJECT: &'static str;

    fn subject_id(&self) -> String;

    /// Whatever a person reading the log a year from now would want, minus
    /// anything that would be a leak: no password hash, no token, no card.
    fn summary(&self) -> serde_json::Value;
}

/// Proof that a change wrote down what it did. Only the two recording
/// functions make one, and [`Audited`] is the only way a mutation can answer,
/// so a mutation that wrote nothing cannot reply with anything.
#[derive(Clone, Copy, Debug)]
pub struct Receipt(());

/// What a mutation answers with. Carries the receipt into the response, where
/// the router checks for it.
#[derive(Debug)]
pub struct Audited<T>(Receipt, pub T);

impl<T> Audited<T> {
    pub fn new(receipt: Receipt, value: T) -> Self {
        Self(receipt, value)
    }
}

impl Receipt {
    /// For the console, which writes to its own log rather than to a tenant's:
    /// what an operator does is about every site or about none.
    #[must_use]
    pub fn for_the_console() -> Self {
        Self(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Wrote;

impl<T: axum::response::IntoResponse> axum::response::IntoResponse for Audited<T> {
    fn into_response(self) -> axum::response::Response {
        let mut response = self.1.into_response();
        response.extensions_mut().insert(Wrote);
        response
    }
}

/// Written in the caller's transaction: an audit line cannot survive a change
/// that rolled back, and the change cannot commit without its line.
pub async fn record<T: Auditable>(
    conn: &mut TenantConn,
    actor: Actor,
    action: &str,
    before: Option<&T>,
    after: Option<&T>,
) -> Result<Receipt> {
    let subject_id = after
        .map(Auditable::subject_id)
        .or_else(|| before.map(Auditable::subject_id));

    write(
        conn,
        actor,
        action,
        T::SUBJECT,
        subject_id.as_deref(),
        before.map(Auditable::summary),
        after.map(Auditable::summary),
    )
    .await
}

/// For what is not a domain type: a sign-in, a refusal, a role change.
pub async fn record_raw(
    conn: &mut TenantConn,
    actor: Actor,
    action: &str,
    subject: &str,
    subject_id: Option<&str>,
    detail: &impl Serialize,
) -> Result<Receipt> {
    let detail = serde_json::to_value(detail).unwrap_or(serde_json::Value::Null);

    write(conn, actor, action, subject, subject_id, None, Some(detail)).await
}

async fn write(
    conn: &mut TenantConn,
    actor: Actor,
    action: &str,
    subject: &str,
    subject_id: Option<&str>,
    before: Option<serde_json::Value>,
    after: Option<serde_json::Value>,
) -> Result<Receipt> {
    let tenant = conn.tenant();

    sqlx::query(
        "insert into audit_log
             (tenant_id, actor_id, actor_kind, action, subject, subject_id,
              before, after, request_id)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(tenant.0)
    .bind(actor.id)
    .bind(actor.kind.as_str())
    .bind(action)
    .bind(subject)
    .bind(subject_id)
    .bind(before)
    .bind(after)
    .bind(actor.request_id.0.to_string())
    .execute(conn.conn())
    .await?;

    Ok(Receipt(()))
}
