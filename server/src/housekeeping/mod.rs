//! What the machine does to itself while nobody is looking.
//!
//! Emptying what has been in the bin long enough, forgetting what a site said
//! it would forget, and sweeping what a day's counting left behind. Everything
//! here is scheduled rather than triggered, and everything is safe to run
//! twice.
use serde::{Deserialize, Serialize};

use crate::kernel::error::Result;
use crate::kernel::http::AppState;
use crate::kernel::queue::Task;
use crate::kernel::retention::{Keeps, POLICIES};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SweepSessions;

impl Task for SweepSessions {
    const KIND: &'static str = "sessions.sweep";
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SweepAudit;

impl Task for SweepAudit {
    const KIND: &'static str = "audit.sweep";
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SweepDeliveries;

impl Task for SweepDeliveries {
    const KIND: &'static str = "webhooks.sweep";
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SweepReports;

impl Task for SweepReports {
    const KIND: &'static str = "reports.sweep";
}

#[must_use]
pub fn kinds() -> Vec<String> {
    vec![
        SweepSessions::KIND.to_owned(),
        SweepAudit::KIND.to_owned(),
        SweepDeliveries::KIND.to_owned(),
        SweepReports::KIND.to_owned(),
    ]
}

fn days(table: &str) -> i32 {
    POLICIES
        .iter()
        .find(|policy| policy.table == table)
        .map_or(30, |policy| match policy.keeps {
            Keeps::Days(days) => days,
            _ => 30,
        })
}

/// A session that has expired is not a session, and a ticket that was never
/// spent is not one either. Both are somebody's, so both go — along with the
/// sign-ins somebody started somewhere else and never came back from.
pub async fn sweep_sessions(state: &AppState) -> Result<u64> {
    let mut conn = state.db.begin().await?;

    let sessions = sqlx::query(
        "delete from sessions
          where expires_at < now() - make_interval(days => $1)
             or (revoked_at is not null and revoked_at < now() - make_interval(days => $1))",
    )
    .bind(days("sessions"))
    .execute(conn.conn())
    .await?
    .rows_affected();

    let students = sqlx::query(
        "delete from student_sessions
          where expires_at < now() - make_interval(days => $1)
             or (revoked_at is not null and revoked_at < now() - make_interval(days => $1))",
    )
    .bind(days("student_sessions"))
    .execute(conn.conn())
    .await?
    .rows_affected();

    let tickets = sqlx::query(
        "delete from tickets
          where expires_at < now() - make_interval(days => $1)
             or (spent_at is not null and spent_at < now() - make_interval(days => $1))",
    )
    .bind(days("tickets"))
    .execute(conn.conn())
    .await?
    .rows_affected();

    // A sign-in somebody began and never finished. Nothing here is worth
    // keeping once it can no longer be answered.
    let attempts =
        sqlx::query("delete from oauth_attempts where expires_at < now() - interval '1 day'")
            .execute(conn.conn())
            .await?
            .rows_affected();

    conn.commit().await?;

    Ok(sessions + students + tickets + attempts)
}

pub async fn sweep_audit(state: &AppState) -> Result<u64> {
    let mut conn = state.db.begin().await?;

    let taken =
        sqlx::query("delete from audit_log where created_at < now() - make_interval(days => $1)")
            .bind(days("audit_log"))
            .execute(conn.conn())
            .await?
            .rows_affected();

    conn.commit().await?;

    Ok(taken)
}

/// A report is kept the same length of time an audit row is: long enough to
/// answer "was this looked at", not forever.
pub async fn sweep_reports(state: &AppState) -> Result<u64> {
    let mut conn = state.db.begin().await?;

    let taken =
        sqlx::query("delete from reports where created_at < now() - make_interval(days => $1)")
            .bind(days("reports"))
            .execute(conn.conn())
            .await?
            .rows_affected();

    conn.commit().await?;

    Ok(taken)
}

/// What was sent and what came back, kept long enough to answer "did it arrive"
/// and no longer.
pub async fn sweep_deliveries(state: &AppState) -> Result<u64> {
    let mut conn = state.db.begin().await?;

    let taken = sqlx::query(
        "delete from webhook_deliveries where sent_at < now() - make_interval(days => $1)",
    )
    .bind(days("webhook_deliveries"))
    .execute(conn.conn())
    .await?
    .rows_affected();

    conn.commit().await?;

    Ok(taken)
}
