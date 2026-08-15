//! What happens by itself when something happens.
//!
//! One trigger and steps in order: no branches, no conditions — two triggers
//! is two flows, and a flow that reads like a program is a program somebody
//! has to debug through a web page. A step that waits does not hold a worker;
//! it goes back into the queue with a moment to come back at.
use axum::Json;
use axum::extract::{Path, State as Injected};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::TenantId;
use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::db::TenantConn;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::page::{Page, Query as Paging, older_than};
use crate::kernel::queue::{self, Task};
use crate::kernel::say::{self, Say};
use crate::kernel::types::Title;
use crate::kernel::{crypto, secret::Secret};

fn flows(access: Access) -> Needs {
    Needs::new(Capability::Flows, access)
}

#[must_use]
#[expect(clippy::too_many_lines, reason = "one list of what is served")]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/flows",
            Guard {
                audience: Audience::User,
                needs: Some(flows(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Flow>>(),
        Endpoint::post(
            "/api/flows",
            Guard {
                audience: Audience::User,
                needs: Some(flows(Access::Write)),
                rate: RatePolicy::None,
            },
            create,
        )
        .takes::<NewFlow>()
        .gives::<Flow>(),
        Endpoint::get(
            "/api/flows/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(flows(Access::View)),
                rate: RatePolicy::None,
            },
            one,
        )
        .gives::<Whole>(),
        Endpoint::patch(
            "/api/flows/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(flows(Access::Write)),
                rate: RatePolicy::None,
            },
            change,
        )
        .takes::<FlowChanges>()
        .gives::<Whole>(),
        Endpoint::delete(
            "/api/flows/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(flows(Access::Delete)),
                rate: RatePolicy::None,
            },
            remove,
        ),
        Endpoint::get(
            "/api/flows/{id}/runs",
            Guard {
                audience: Audience::User,
                needs: Some(flows(Access::View)),
                rate: RatePolicy::None,
            },
            runs,
        )
        .gives::<Page<Run>>(),
        Endpoint::get(
            "/api/flows/runs/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(flows(Access::View)),
                rate: RatePolicy::None,
            },
            run,
        )
        .gives::<WholeRun>(),
        Endpoint::get(
            "/api/flows/credentials",
            Guard {
                audience: Audience::User,
                needs: Some(flows(Access::View)),
                rate: RatePolicy::None,
            },
            credentials,
        )
        .gives::<Vec<Credential>>(),
        Endpoint::post(
            "/api/flows/credentials",
            Guard {
                audience: Audience::User,
                needs: Some(flows(Access::Write)),
                rate: RatePolicy::None,
            },
            keep_credential,
        )
        .takes::<NewCredential>(),
        Endpoint::delete(
            "/api/flows/credentials/{name}",
            Guard {
                audience: Audience::User,
                needs: Some(flows(Access::Delete)),
                rate: RatePolicy::None,
            },
            forget_credential,
        ),
    ]
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Flow {
    pub id: Uuid,
    pub name: String,
    pub trigger: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Run {
    pub id: Uuid,
    pub state: String,
    pub at_step: i32,
    pub failure: Option<String>,
    pub started_at: DateTime<Utc>,
}

/// What one step of a run did, and what it said if it failed.
#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Taken {
    pub position: i32,
    pub kind: Option<StepKind>,
    /// `went on`, `waiting` or `failed`.
    pub outcome: String,
    pub detail: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// A run and every step it took.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[schema(as = WholeRun)]
pub struct WholeRun {
    pub run: Run,
    pub steps: Vec<Taken>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "step_kind", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    SendMail,
    CallWebhook,
    Wait,
    AddToList,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewFlow {
    pub name: Title,
    /// The event that starts it. Checked against what this build can emit, so
    /// a flow waiting for something nothing sends is refused rather than
    /// quietly never running.
    pub trigger: String,
    #[serde(default)]
    pub steps: Vec<NewStep>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewStep {
    pub kind: StepKind,
    #[serde(default)]
    pub config: serde_json::Map<String, serde_json::Value>,
}

/// One credential a site has kept, by name. Never the secret: nothing in this
/// API answers with one, which is the whole reason there is a name at all.
#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Credential {
    pub name: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewCredential {
    pub name: Title,
    /// Sealed on the way in and never handed back. Nothing in this API returns
    /// one, which is why there is no endpoint that reads them.
    pub secret: Secret<String>,
}

/// Every event a flow may wait for. A trigger that is not here is refused when
/// the flow is made, because a flow waiting for something nothing emits looks
/// exactly like a flow that is broken.
#[must_use]
pub fn triggers() -> Vec<&'static str> {
    vec![
        "form.submitted",
        "post.published",
        "post.unpublished",
        "order.paid",
        "order.fulfilled",
        "refund.made",
        "stock.low",
    ]
}

async fn list(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    axum::extract::Query(page): axum::extract::Query<Paging>,
) -> Result<Json<Page<Flow>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Flow> = sqlx::query_as(
        "select id, name, trigger, active, created_at from flows
          where deleted_at is null
            and ($1::timestamptz is null or created_at < $1)
          order by created_at desc
          limit $2",
    )
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&page, rows, |flow| {
        flow.created_at.to_rfc3339()
    })))
}

async fn create(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<NewFlow>,
) -> Result<Audited<(StatusCode, Json<Flow>)>> {
    if !triggers().contains(&body.trigger.as_str()) {
        return Err(AppError::Invalid(
            Say::of(say::NOTHING_HERE_EMITS_THAT).naming("event", &body.trigger),
        ));
    }

    let mut conn = state.db.tenant(caller.tenant()).await?;

    let flow: Flow = sqlx::query_as(
        "insert into flows (tenant_id, name, trigger, active) values ($1, $2, $3, true)
         returning id, name, trigger, active, created_at",
    )
    .bind(caller.tenant().0)
    .bind(body.name.as_str())
    .bind(&body.trigger)
    .fetch_one(conn.conn())
    .await?;

    for (position, step) in body.steps.iter().enumerate() {
        sqlx::query(
            "insert into flow_steps (tenant_id, flow_id, kind, config, position)
             values ($1, $2, $3, $4, $5)",
        )
        .bind(caller.tenant().0)
        .bind(flow.id)
        .bind(step.kind)
        .bind(serde_json::Value::Object(step.config.clone()))
        .bind(i32::try_from(position).unwrap_or(i32::MAX))
        .execute(conn.conn())
        .await?;
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "made a flow",
        "flow",
        Some(&flow.id.to_string()),
        &serde_json::json!({ "trigger": flow.trigger, "steps": body.steps.len() }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(flow))))
}

/// A flow and the steps in it, in order.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[schema(as = WholeFlow)]
pub struct Whole {
    pub flow: Flow,
    pub steps: Vec<Written>,
}

/// One step of a flow, as a screen reads and writes it.
#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
#[schema(as = FlowStep)]
pub struct Written {
    pub id: Uuid,
    pub kind: StepKind,
    pub config: serde_json::Value,
    pub position: i32,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowChanges {
    pub name: Option<Title>,
    pub active: Option<bool>,
    /// The whole list, in order, or left out to leave the steps alone. A flow
    /// is what it does in order, and sending one step would mean nobody could
    /// take one out.
    pub steps: Option<Vec<NewStep>>,
}

async fn one(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Json<Whole>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    let whole = read(&mut conn, id).await?;
    conn.commit().await?;

    Ok(Json(whole))
}

async fn read(conn: &mut crate::kernel::db::TenantConn, id: Uuid) -> Result<Whole> {
    let flow: Option<Flow> = sqlx::query_as(
        "select id, name, trigger, active, created_at
           from flows where id = $1 and deleted_at is null",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?;

    let flow = flow.ok_or(AppError::NotFound("flow"))?;

    let steps: Vec<Written> = sqlx::query_as(
        "select id, kind, config, position from flow_steps
          where flow_id = $1 order by position",
    )
    .bind(id)
    .fetch_all(conn.conn())
    .await?;

    Ok(Whole { flow, steps })
}

/// Changing what a flow is called, whether it runs, and what it does.
///
/// The steps are written whole: what is here now goes and what was sent takes
/// its place, in the order it was sent. A flow half-rewritten while it is
/// running is what the run's own copy of its step number protects against.
async fn change(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    Json(wanted): Json<FlowChanges>,
) -> Result<Audited<Json<Whole>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let changed = sqlx::query(
        "update flows set name = coalesce($2, name), active = coalesce($3, active)
          where id = $1 and deleted_at is null",
    )
    .bind(id)
    .bind(wanted.name.as_ref().map(Title::as_str))
    .bind(wanted.active)
    .execute(conn.conn())
    .await?
    .rows_affected();

    if changed == 0 {
        return Err(AppError::NotFound("flow"));
    }

    if let Some(steps) = &wanted.steps {
        sqlx::query("delete from flow_steps where flow_id = $1")
            .bind(id)
            .execute(conn.conn())
            .await?;

        for (position, step) in steps.iter().enumerate() {
            sqlx::query(
                "insert into flow_steps (tenant_id, flow_id, kind, config, position)
                 values ($1, $2, $3, $4, $5)",
            )
            .bind(caller.tenant().0)
            .bind(id)
            .bind(step.kind)
            .bind(serde_json::Value::Object(step.config.clone()))
            .bind(i32::try_from(position).unwrap_or(i32::MAX))
            .execute(conn.conn())
            .await?;
        }
    }

    let whole = read(&mut conn, id).await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "changed a flow",
        "flow",
        Some(&id.to_string()),
        &serde_json::json!({ "active": whole.flow.active, "steps": whole.steps.len() }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(whole)))
}

/// Taking a flow away. What it already did stays in the record, and the runs
/// go with it: a run of a flow nobody has is a row nothing can explain.
async fn remove(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let gone = sqlx::query(
        "update flows set deleted_at = now(), active = false
          where id = $1 and deleted_at is null",
    )
    .bind(id)
    .execute(conn.conn())
    .await?
    .rows_affected();

    if gone == 0 {
        return Err(AppError::NotFound("flow"));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "took a flow away",
        "flow",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

async fn runs(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    axum::extract::Query(page): axum::extract::Query<Paging>,
) -> Result<Json<Page<Run>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Run> = sqlx::query_as(
        "select id, state::text as state, at_step, failure, started_at
           from flow_runs
          where flow_id = $1
            and ($2::timestamptz is null or started_at < $2)
          order by started_at desc
          limit $3",
    )
    .bind(id)
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&page, rows, |run| {
        run.started_at.to_rfc3339()
    })))
}

/// One run, and what each of its steps did.
///
/// The list of runs says a run failed; this says which step it failed at and
/// what the step said, which is the difference between knowing something is
/// wrong and being able to fix it.
async fn run(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Json<WholeRun>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let run: Run = sqlx::query_as(
        "select id, state::text as state, at_step, failure, started_at
           from flow_runs where id = $1",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?
    .ok_or(AppError::NotFound("run"))?;

    let steps: Vec<Taken> = sqlx::query_as(
        "select taken.position, step.kind, taken.outcome, taken.detail, taken.created_at
           from flow_run_steps taken
           left join flow_steps step on step.id = taken.step_id
          where taken.run_id = $1
          order by taken.position",
    )
    .bind(id)
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(WholeRun { run, steps }))
}

/// Which credentials this site has kept — by name, and when each was last
/// written. A site that cannot see them cannot tell a stale one from a missing
/// one, and both look the same when a flow fails.
async fn credentials(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
) -> Result<Json<Vec<Credential>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Credential> =
        sqlx::query_as("select name, updated_at from flow_credentials order by name")
            .fetch_all(conn.conn())
            .await?;

    conn.commit().await?;

    Ok(Json(rows))
}

/// Taking one away, for a service a site has stopped using. What a flow that
/// still wants it gets is a run that fails saying so, which is the honest
/// answer.
async fn forget_credential(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(name): Path<String>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let gone = sqlx::query("delete from flow_credentials where name = $1")
        .bind(&name)
        .execute(conn.conn())
        .await?
        .rows_affected();

    if gone == 0 {
        return Err(AppError::NotFound("credential"));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "took away a credential",
        "flow_credential",
        Some(&name),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

async fn keep_credential(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<NewCredential>,
) -> Result<Audited<StatusCode>> {
    let sealed = crypto::seal(&state.keyring, body.secret.expose())?;
    let mut conn = state.db.tenant(caller.tenant()).await?;

    sqlx::query(
        "insert into flow_credentials (tenant_id, name, sealed) values ($1, $2, $3)
         on conflict (tenant_id, name) do update set sealed = excluded.sealed",
    )
    .bind(caller.tenant().0)
    .bind(body.name.as_str())
    .bind(&sealed)
    .execute(conn.conn())
    .await?;

    // The name, never the secret, and never any part of it.
    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "kept a credential",
        "flow_credential",
        Some(body.name.as_str()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

pub async fn credential(
    state: &AppState,
    conn: &mut TenantConn,
    name: &str,
) -> Result<Secret<String>> {
    let found: Option<(String,)> =
        sqlx::query_as("select sealed from flow_credentials where name = $1")
            .bind(name)
            .fetch_optional(conn.conn())
            .await?;

    let (sealed,) = found.ok_or(AppError::NotFound("credential"))?;

    crypto::open(&state.keyring, &sealed)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Start {
    pub outbox_id: Uuid,
}

impl Task for Start {
    const KIND: &'static str = "flow.start";
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Step {
    pub run_id: Uuid,
}

impl Task for Step {
    const KIND: &'static str = "flow.step";
}

#[must_use]
pub fn kinds() -> Vec<String> {
    vec![Start::KIND.to_owned(), Step::KIND.to_owned()]
}

/// An event arrived; whichever flows were waiting for it get a run each.
pub async fn start(state: &AppState, tenant: TenantId, task: &Start) -> Result<u64> {
    let mut conn = state.db.tenant(tenant).await?;

    let event: Option<(String, serde_json::Value)> =
        sqlx::query_as("select event, payload from outbox where id = $1")
            .bind(task.outbox_id)
            .fetch_optional(conn.conn())
            .await?;

    let Some((event, payload)) = event else {
        return Ok(0);
    };

    let waiting: Vec<(Uuid,)> =
        sqlx::query_as("select id from flows where trigger = $1 and active and deleted_at is null")
            .bind(&event)
            .fetch_all(conn.conn())
            .await?;

    for (flow_id,) in &waiting {
        let run: (Uuid,) = sqlx::query_as(
            "insert into flow_runs (tenant_id, flow_id, subject) values ($1, $2, $3)
             returning id",
        )
        .bind(tenant.0)
        .bind(flow_id)
        .bind(&payload)
        .fetch_one(conn.conn())
        .await?;

        queue::enqueue(&mut conn, &Step { run_id: run.0 }, None).await?;
    }

    conn.commit().await?;

    Ok(waiting.len() as u64)
}

/// One step, then the next as its own piece of work.
///
/// A step at a time rather than a run at a time: a flow that waits an hour does
/// not hold a worker for an hour, and a step that fails is retried on its own
/// rather than repeating everything before it.
pub async fn step(state: &AppState, tenant: TenantId, task: &Step) -> Result<()> {
    let mut conn = state.db.tenant(tenant).await?;

    let run: Option<(Uuid, i32, serde_json::Value, String)> = sqlx::query_as(
        "select flow_id, at_step, subject, state::text from flow_runs
          where id = $1 for update",
    )
    .bind(task.run_id)
    .fetch_optional(conn.conn())
    .await?;

    let Some((flow_id, at_step, subject, state_of)) = run else {
        return Ok(());
    };

    if state_of == "done" || state_of == "failed" {
        return Ok(());
    }

    let next: Option<(Uuid, StepKind, serde_json::Value)> = sqlx::query_as(
        "select id, kind, config from flow_steps where flow_id = $1 and position = $2",
    )
    .bind(flow_id)
    .bind(at_step)
    .fetch_optional(conn.conn())
    .await?;

    let Some((step_id, kind, config)) = next else {
        sqlx::query("update flow_runs set state = 'done', finished_at = now() where id = $1")
            .bind(task.run_id)
            .execute(conn.conn())
            .await?;

        conn.commit().await?;

        return Ok(());
    };

    let outcome = run_step(state, &mut conn, tenant, kind, &config, &subject).await;

    sqlx::query(
        "insert into flow_run_steps (tenant_id, run_id, step_id, position, outcome, detail)
         values ($1, $2, $3, $4, $5, $6)",
    )
    .bind(tenant.0)
    .bind(task.run_id)
    .bind(step_id)
    .bind(at_step)
    .bind(match &outcome {
        Ok(Went::On) => "went on",
        Ok(Went::Waiting(_)) => "waiting",
        Err(_) => "failed",
    })
    .bind(match &outcome {
        Err(why) => serde_json::json!({ "why": why.to_string() }),
        _ => serde_json::json!({}),
    })
    .execute(conn.conn())
    .await?;

    match outcome {
        Ok(went) => {
            let after = match went {
                Went::Waiting(until) => Some(until),
                Went::On => None,
            };

            sqlx::query("update flow_runs set at_step = at_step + 1 where id = $1")
                .bind(task.run_id)
                .execute(conn.conn())
                .await?;

            queue::enqueue(
                &mut conn,
                &Step {
                    run_id: task.run_id,
                },
                after,
            )
            .await?;
        }
        Err(why) => {
            // A run that failed stays failed and stays readable: what went
            // wrong on which step is the question a person asks.
            sqlx::query(
                "update flow_runs set state = 'failed', failure = $2, finished_at = now()
                  where id = $1",
            )
            .bind(task.run_id)
            .bind(why.to_string())
            .execute(conn.conn())
            .await?;
        }
    }

    conn.commit().await?;

    Ok(())
}

enum Went {
    On,
    Waiting(DateTime<Utc>),
}

async fn run_step(
    state: &AppState,
    conn: &mut TenantConn,
    tenant: TenantId,
    kind: StepKind,
    config: &serde_json::Value,
    subject: &serde_json::Value,
) -> Result<Went> {
    match kind {
        StepKind::Wait => {
            let minutes = config
                .get("minutes")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(60)
                .clamp(1, 60 * 24 * 30);

            Ok(Went::Waiting(
                state.clock.now() + Duration::minutes(minutes),
            ))
        }
        StepKind::SendMail => {
            let to = config
                .get("to")
                .and_then(serde_json::Value::as_str)
                .or_else(|| subject.get("email").and_then(serde_json::Value::as_str))
                .ok_or_else(|| AppError::Invalid(say::STEP_NOBODY_WRITE.into()))?;

            let subject_line = config
                .get("subject")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("A message from the site");

            sqlx::query("insert into email_log (tenant_id, to_email, subject) values ($1, $2, $3)")
                .bind(tenant.0)
                .bind(to)
                .bind(subject_line)
                .execute(conn.conn())
                .await?;

            Ok(Went::On)
        }
        StepKind::AddToList => {
            let list = config
                .get("list_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|id| id.parse::<Uuid>().ok())
                .ok_or_else(|| AppError::Invalid(say::STEP_NO_LIST.into()))?;

            let email = config
                .get("email")
                .and_then(serde_json::Value::as_str)
                .or_else(|| subject.get("email").and_then(serde_json::Value::as_str))
                .ok_or_else(|| AppError::Invalid(say::STEP_NOBODY_ADD.into()))?;

            let subscriber: (Uuid,) = sqlx::query_as(
                "insert into subscribers (tenant_id, email, token_hash)
                 values ($1, $2, sha256(gen_random_uuid()::text::bytea))
                 on conflict (tenant_id, email) do update set updated_at = now()
                 returning id",
            )
            .bind(tenant.0)
            .bind(email)
            .fetch_one(conn.conn())
            .await?;

            sqlx::query(
                "insert into subscriber_lists (subscriber_id, list_id, tenant_id)
                 values ($1, $2, $3) on conflict do nothing",
            )
            .bind(subscriber.0)
            .bind(list)
            .bind(tenant.0)
            .execute(conn.conn())
            .await?;

            Ok(Went::On)
        }
        StepKind::CallWebhook => {
            // Deliberately the outbox rather than a call from here: a step that
            // waits on somebody else's server is a step holding a transaction
            // open, and delivery already knows how to retry and where not to
            // send.
            let event = config
                .get("event")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("flow.step");

            sqlx::query("insert into outbox (tenant_id, event, payload) values ($1, $2, $3)")
                .bind(tenant.0)
                .bind(event)
                .bind(subject)
                .execute(conn.conn())
                .await?;

            Ok(Went::On)
        }
    }
}
