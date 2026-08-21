//! Site-scoped trigger-driven automation.
//!
//! A flow is a validated immutable-at-run-time definition: one trigger and a
//! bounded ordered list of steps. Emitting an event only enqueues durable work;
//! it never performs a provider call in the request transaction. A run stores
//! the definition snapshot that it started with, so editing or deleting a flow
//! cannot change a customer-facing run halfway through.

use std::net::IpAddr;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Cursor, ErrorCode, FlowId, FlowRunId, FlowRunStepId, FlowStepId, JobId,
    MaviError, Page, PageRequest, Result, SiteContext,
};
use mavi_jobs::{JobKind, JobsService};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, QueryBuilder, Row};
use uuid::Uuid;

mod relocation;

pub use relocation::{
    FlowRelocation, FlowRunRelocation, FlowRunStepRelocation, FlowStepRelocation, FlowsRelocation,
};

pub const FLOW_START_KIND: JobKind = JobKind::new("automation.flow.start", 5);
pub const FLOW_STEP_KIND: JobKind = JobKind::new("automation.flow.step", 5);
pub const MAX_FLOW_STEPS: usize = 32;
pub const MAX_FLOW_NAME: usize = 200;
pub const MAX_WAIT_SECONDS: i64 = 2_678_400;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    ContentPublished,
    FormSubmitted,
    OrderPaid,
    OrderSent,
    CourseEnrollmentCreated,
    CourseLessonCompleted,
}

impl Trigger {
    pub const ALL: [Self; 6] = [
        Self::ContentPublished,
        Self::FormSubmitted,
        Self::OrderPaid,
        Self::OrderSent,
        Self::CourseEnrollmentCreated,
        Self::CourseLessonCompleted,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContentPublished => "content_published",
            Self::FormSubmitted => "form_submitted",
            Self::OrderPaid => "order_paid",
            Self::OrderSent => "order_sent",
            Self::CourseEnrollmentCreated => "course_enrollment_created",
            Self::CourseLessonCompleted => "course_lesson_completed",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    SendMail,
    Webhook,
    Wait,
    AddToMailList,
}

impl StepKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SendMail => "send_mail",
            Self::Webhook => "webhook",
            Self::Wait => "wait",
            Self::AddToMailList => "add_to_mail_list",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowStepInput {
    pub kind: StepKind,
    #[serde(default)]
    pub config: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateFlow {
    pub name: String,
    pub trigger: Trigger,
    pub steps: Vec<FlowStepInput>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateFlow {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub trigger: Option<Trigger>,
    pub steps: Option<Vec<FlowStepInput>>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub trigger: Option<Trigger>,
    pub enabled: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FlowStep {
    pub id: FlowStepId,
    pub position: i32,
    pub kind: StepKind,
    pub config: Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct Flow {
    pub id: FlowId,
    pub name: String,
    pub trigger: Trigger,
    pub enabled: bool,
    pub version: i32,
    pub steps: Vec<FlowStep>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Running,
    Waiting,
    Succeeded,
    Failed,
}

impl RunState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "running" => Ok(Self::Running),
            "waiting" => Ok(Self::Waiting),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            _ => Err(MaviError::Internal),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub state: Option<RunState>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FlowRunStep {
    pub id: FlowRunStepId,
    pub position: i32,
    pub attempt: i32,
    pub kind: StepKind,
    pub outcome: String,
    pub detail: Value,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FlowRun {
    pub id: FlowRunId,
    pub flow_id: FlowId,
    pub trigger: Trigger,
    pub event: Value,
    pub definition: Vec<FlowStepInput>,
    pub state: RunState,
    pub current_position: i32,
    pub retry_count: i32,
    pub last_error: Option<String>,
    pub steps: Vec<FlowRunStep>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SimulateFlow {
    #[serde(default)]
    pub event: Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct SimulationStep {
    pub position: i32,
    pub kind: StepKind,
    pub config: Value,
    pub event: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StartFlowJob {
    pub flow_id: FlowId,
    pub trigger: Trigger,
    pub event: Value,
    pub source_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StepJob {
    pub run_id: FlowRunId,
    pub position: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepOutcome {
    Succeeded,
    Waiting,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RecordStep {
    pub run_id: FlowRunId,
    pub position: i32,
    pub attempt: i32,
    pub outcome: StepOutcome,
    #[serde(default)]
    pub detail: Value,
    pub error: Option<String>,
    pub next_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TriggerDescription {
    pub trigger: Trigger,
    pub emitted_by: &'static str,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FlowService;

#[must_use]
pub fn job_kinds() -> [JobKind; 2] {
    [FLOW_START_KIND, FLOW_STEP_KIND]
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn api() -> mavi_contract::Api {
    let view = Permission {
        capability: Capability::Automation,
        action: Action::View,
    };
    let write = Permission {
        capability: Capability::Automation,
        action: Action::Write,
    };
    mavi_contract::Api::new(vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/automation/triggers",
            "automation.triggers.list",
            "List supported automation triggers",
        )
        .account_or_assistant()
        .requires(view)
        .returns(200, "TriggerList")
        .refuses([ErrorCode::Forbidden, ErrorCode::Internal]),
        Endpoint::new(
            Method::Get,
            "/api/v1/automation/flows",
            "automation.flows.list",
            "List automation flows with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("FlowListFilter")
        .returns(200, "FlowPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/automation/flows",
            "automation.flows.create",
            "Create a validated automation flow",
        )
        .account_or_assistant()
        .requires(write)
        .takes("CreateFlow")
        .returns(201, "Flow")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/automation/flows/{id}",
            "automation.flows.read",
            "Read one automation flow",
        )
        .account_or_assistant()
        .requires(view)
        .returns(200, "Flow")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/automation/flows/{id}",
            "automation.flows.update",
            "Update a flow definition or enablement",
        )
        .account_or_assistant()
        .requires(write)
        .takes("UpdateFlow")
        .returns(200, "Flow")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/automation/flows/{id}",
            "automation.flows.delete",
            "Move a flow definition to site trash",
        )
        .account_or_assistant()
        .requires(write)
        .returns(204, "Empty")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/automation/flows/{id}/simulate",
            "automation.flows.simulate",
            "Preview flow steps without enqueueing work",
        )
        .account_or_assistant()
        .requires(view)
        .takes("SimulateFlow")
        .returns(200, "Simulation")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/automation/flows/{id}/runs",
            "automation.runs.list",
            "List runs for a flow with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("RunListFilter")
        .returns(200, "FlowRunPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/automation/runs/{id}",
            "automation.runs.read",
            "Read one automation run and its step history",
        )
        .account_or_assistant()
        .requires(view)
        .returns(200, "FlowRun")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
    ])
    .with_shapes(shapes())
}

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "Trigger",
            json!({"type":"string","enum":["content_published","form_submitted","order_paid","order_sent","course_enrollment_created","course_lesson_completed"]}),
        ),
        Shape::new(
            "StepKind",
            json!({"type":"string","enum":["send_mail","webhook","wait","add_to_mail_list"]}),
        ),
        Shape::new(
            "RunState",
            json!({"type":"string","enum":["running","waiting","succeeded","failed"]}),
        ),
        Shape::new(
            "FlowStepInput",
            json!({"type":"object","required":["kind","config"],"properties":{"kind":{"$ref":"#/components/schemas/StepKind"},"config":{"type":"object","additionalProperties":true}}}),
        ),
        Shape::new(
            "CreateFlow",
            json!({"type":"object","required":["name","trigger","steps"],"properties":{"name":{"type":"string","minLength":1,"maxLength":200},"trigger":{"$ref":"#/components/schemas/Trigger"},"steps":{"type":"array","minItems":1,"maxItems":32,"items":{"$ref":"#/components/schemas/FlowStepInput"}}}}),
        ),
        Shape::new(
            "UpdateFlow",
            json!({"type":"object","properties":{"name":{"type":["string","null"],"maxLength":200},"enabled":{"type":["boolean","null"]},"trigger":{"anyOf":[{"$ref":"#/components/schemas/Trigger"},{"type":"null"}]},"steps":{"type":["array","null"],"maxItems":32,"items":{"$ref":"#/components/schemas/FlowStepInput"}}}}),
        ),
        Shape::new(
            "FlowListFilter",
            json!({"type":"object","properties":{"after":{"type":["string","null"],"maxLength":512},"limit":{"type":"integer","minimum":1,"maximum":100},"trigger":{"anyOf":[{"$ref":"#/components/schemas/Trigger"},{"type":"null"}]},"enabled":{"type":["boolean","null"]}}}),
        ),
        Shape::new(
            "FlowStep",
            json!({"type":"object","required":["id","position","kind","config"],"properties":{"id":{"type":"string","format":"uuid"},"position":{"type":"integer","minimum":0},"kind":{"$ref":"#/components/schemas/StepKind"},"config":{"type":"object","additionalProperties":true}}}),
        ),
        Shape::new(
            "Flow",
            json!({"type":"object","required":["id","name","trigger","enabled","version","steps","created_at","updated_at"],"properties":{"id":{"type":"string","format":"uuid"},"name":{"type":"string"},"trigger":{"$ref":"#/components/schemas/Trigger"},"enabled":{"type":"boolean"},"version":{"type":"integer"},"steps":{"type":"array","items":{"$ref":"#/components/schemas/FlowStep"}},"created_at":{"type":"string","format":"date-time"},"updated_at":{"type":"string","format":"date-time"}}}),
        ),
        Shape::new(
            "FlowPage",
            json!({"type":"object","required":["items","next_cursor"],"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/Flow"}},"next_cursor":{"type":["string","null"]}}}),
        ),
        Shape::new(
            "RunListFilter",
            json!({"type":"object","properties":{"after":{"type":["string","null"],"maxLength":512},"limit":{"type":"integer","minimum":1,"maximum":100},"state":{"anyOf":[{"$ref":"#/components/schemas/RunState"},{"type":"null"}]}}}),
        ),
        Shape::new(
            "FlowRunStep",
            json!({"type":"object","required":["id","position","attempt","kind","outcome","detail","error","started_at","finished_at"],"properties":{"id":{"type":"string","format":"uuid"},"position":{"type":"integer"},"attempt":{"type":"integer"},"kind":{"$ref":"#/components/schemas/StepKind"},"outcome":{"type":"string"},"detail":{"type":"object","additionalProperties":true},"error":{"type":["string","null"]},"started_at":{"type":"string","format":"date-time"},"finished_at":{"type":"string","format":"date-time"}}}),
        ),
        Shape::new(
            "FlowRun",
            json!({"type":"object","required":["id","flow_id","trigger","event","definition","state","current_position","retry_count","last_error","steps","started_at","finished_at"],"properties":{"id":{"type":"string","format":"uuid"},"flow_id":{"type":"string","format":"uuid"},"trigger":{"$ref":"#/components/schemas/Trigger"},"event":{"type":"object","additionalProperties":true},"definition":{"type":"array","items":{"$ref":"#/components/schemas/FlowStepInput"}},"state":{"$ref":"#/components/schemas/RunState"},"current_position":{"type":"integer"},"retry_count":{"type":"integer"},"last_error":{"type":["string","null"]},"steps":{"type":"array","items":{"$ref":"#/components/schemas/FlowRunStep"}},"started_at":{"type":"string","format":"date-time"},"finished_at":{"type":["string","null"],"format":"date-time"}}}),
        ),
        Shape::new(
            "FlowRunPage",
            json!({"type":"object","required":["items","next_cursor"],"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/FlowRun"}},"next_cursor":{"type":["string","null"]}}}),
        ),
        Shape::new(
            "TriggerDescription",
            json!({"type":"object","required":["trigger","emitted_by"],"properties":{"trigger":{"$ref":"#/components/schemas/Trigger"},"emitted_by":{"type":"string"}}}),
        ),
        Shape::new(
            "TriggerList",
            json!({"type":"array","items":{"$ref":"#/components/schemas/TriggerDescription"}}),
        ),
        Shape::new(
            "SimulateFlow",
            json!({"type":"object","properties":{"event":{"type":"object","additionalProperties":true}}}),
        ),
        Shape::new(
            "SimulationStep",
            json!({"type":"object","required":["position","kind","config","event"],"properties":{"position":{"type":"integer"},"kind":{"$ref":"#/components/schemas/StepKind"},"config":{"type":"object","additionalProperties":true},"event":{"type":"object","additionalProperties":true}}}),
        ),
        Shape::new(
            "Simulation",
            json!({"type":"object","required":["steps"],"properties":{"steps":{"type":"array","items":{"$ref":"#/components/schemas/SimulationStep"}}}}),
        ),
    ]
}

impl FlowService {
    pub async fn create(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateFlow,
    ) -> Result<Flow> {
        let name = validate_name(&input.name)?;
        validate_steps(&input.steps)?;
        let id = FlowId::new();
        sqlx::query(
            "insert into automation_flows
                (site_id, id, name, trigger, enabled, version)
             values ($1, $2, $3, $4, false, 1)",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(&name)
        .bind(input.trigger.as_str())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        insert_steps(tx, context, id, &input.steps).await?;
        audit(
            tx,
            context,
            "automation.flow.created",
            "AutomationFlow",
            id.into_uuid(),
            json!({"trigger": input.trigger.as_str(), "steps": input.steps.len()}),
        )
        .await?;
        self.get(tx, id).await
    }

    pub async fn get(&self, tx: &mut SiteTx, id: FlowId) -> Result<Flow> {
        let row = sqlx::query(
            "select id, name, trigger, enabled, version, created_at, updated_at
               from automation_flows where id = $1 and deleted_at is null",
        )
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: "automation_flow",
        })?;
        let steps = read_steps(tx, id).await?;
        Ok(Flow {
            id,
            name: row.try_get("name").map_err(|_| MaviError::Internal)?,
            trigger: parse_trigger(
                &row.try_get::<String, _>("trigger")
                    .map_err(|_| MaviError::Internal)?,
            )?,
            enabled: row.try_get("enabled").map_err(|_| MaviError::Internal)?,
            version: row.try_get("version").map_err(|_| MaviError::Internal)?,
            steps,
            created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
        })
    }

    pub async fn list(&self, tx: &mut SiteTx, filter: &FlowListFilter) -> Result<Page<Flow>> {
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = QueryBuilder::<Postgres>::new(
            "select id, name, trigger, enabled, version, created_at, updated_at
               from automation_flows where deleted_at is null",
        );
        if let Some(trigger) = filter.trigger {
            query.push(" and trigger = ").push_bind(trigger.as_str());
        }
        if let Some(enabled) = filter.enabled {
            query.push(" and enabled = ").push_bind(enabled);
        }
        if let Some(after) = after {
            query
                .push(" and (created_at, id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        query
            .push(" order by created_at desc, id desc limit ")
            .push_bind(limit + 1);
        let rows = query
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            let id = FlowId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
            items.push(Flow {
                id,
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                trigger: parse_trigger(
                    &row.try_get::<String, _>("trigger")
                        .map_err(|_| MaviError::Internal)?,
                )?,
                enabled: row.try_get("enabled").map_err(|_| MaviError::Internal)?,
                version: row.try_get("version").map_err(|_| MaviError::Internal)?,
                steps: read_steps(tx, id).await?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
            });
        }
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let last = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_cursor(last.created_at, last.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn update(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: FlowId,
        input: &UpdateFlow,
    ) -> Result<Flow> {
        if input.name.is_none()
            && input.enabled.is_none()
            && input.trigger.is_none()
            && input.steps.is_none()
        {
            return Err(MaviError::validation("flow_update_empty"));
        }
        let current = self.get(tx, id).await?;
        let name = input.name.as_deref().map(validate_name).transpose()?;
        if let Some(steps) = &input.steps {
            validate_steps(steps)?;
        }
        sqlx::query(
            "update automation_flows
                set name = coalesce($2, name),
                    trigger = coalesce($3, trigger),
                    enabled = coalesce($4, enabled),
                    version = version + 1, updated_at = now()
              where id = $1 and deleted_at is null",
        )
        .bind(id.into_uuid())
        .bind(name.as_deref())
        .bind(input.trigger.map(Trigger::as_str))
        .bind(input.enabled)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if let Some(steps) = &input.steps {
            sqlx::query("delete from automation_flow_steps where flow_id = $1")
                .bind(id.into_uuid())
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
            insert_steps(tx, context, id, steps).await?;
        }
        audit(
            tx,
            context,
            "automation.flow.updated",
            "AutomationFlow",
            id.into_uuid(),
            json!({"previous_version": current.version}),
        )
        .await?;
        self.get(tx, id).await
    }

    pub async fn delete(&self, tx: &mut SiteTx, context: &SiteContext, id: FlowId) -> Result<()> {
        let rows = sqlx::query(
            "update automation_flows
                set trash_enabled = enabled,
                    deleted_at = now(),
                    enabled = false,
                    updated_at = now()
              where site_id = $1 and id = $2 and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if rows.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: "automation_flow",
            });
        }
        audit(
            tx,
            context,
            "automation.flow.trashed",
            "AutomationFlow",
            id.into_uuid(),
            json!({}),
        )
        .await
    }

    pub async fn simulate(
        &self,
        tx: &mut SiteTx,
        id: FlowId,
        input: &SimulateFlow,
    ) -> Result<Vec<SimulationStep>> {
        let flow = self.get(tx, id).await?;
        Ok(flow
            .steps
            .into_iter()
            .map(|step| SimulationStep {
                position: step.position,
                kind: step.kind,
                config: step.config,
                event: input.event.clone(),
            })
            .collect())
    }

    /// Fan an emitted domain event out to all enabled matching flows. This is
    /// intended to run inside the event producer's transaction.
    pub async fn emit(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        jobs: &JobsService,
        trigger: Trigger,
        event: &Value,
        source_key: Option<&str>,
    ) -> Result<Vec<JobId>> {
        validate_event(event)?;
        let flow_ids: Vec<Uuid> = sqlx::query_scalar(
            "select id from automation_flows
               where trigger = $1 and enabled and deleted_at is null
               order by id",
        )
        .bind(trigger.as_str())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let mut jobs_created = Vec::with_capacity(flow_ids.len());
        for flow_id in flow_ids {
            let payload = json!({
                "flow_id": flow_id,
                "trigger": trigger,
                "event": event,
                "source_key": source_key
            });
            let key = source_key.map(|source| format!("flow:{flow_id}:{source}"));
            jobs_created.push(
                jobs.enqueue(
                    tx,
                    context,
                    FLOW_START_KIND.name,
                    &payload,
                    None,
                    key.as_deref(),
                )
                .await?,
            );
        }
        Ok(jobs_created)
    }

    /// Materialize a queued start into a run and enqueue its first step.
    pub async fn start(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        jobs: &JobsService,
        input: &StartFlowJob,
    ) -> Result<FlowRun> {
        let flow = self.get(tx, input.flow_id).await?;
        if !flow.enabled || flow.trigger != input.trigger {
            return Err(MaviError::conflict("flow_not_enabled_for_trigger"));
        }
        if let Some(source_key) = &input.source_key {
            let existing: Option<Uuid> = sqlx::query_scalar(
                "select id from automation_runs where flow_id = $1 and source_key = $2",
            )
            .bind(input.flow_id.into_uuid())
            .bind(source_key)
            .fetch_optional(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            if let Some(existing) = existing {
                return self.get_run(tx, FlowRunId::from_uuid(existing)).await;
            }
        }
        let run_id = FlowRunId::new();
        let definition = flow
            .steps
            .iter()
            .map(|step| FlowStepInput {
                kind: step.kind,
                config: step.config.clone(),
            })
            .collect::<Vec<_>>();
        let inserted = sqlx::query(
            "insert into automation_runs
                (site_id, id, flow_id, trigger, source_key, event, definition, state, current_position, retry_count)
             values ($1, $2, $3, $4, $5, $6, $7, 'running', 0, 0)
             on conflict (site_id, flow_id, source_key)
                 where source_key is not null do nothing
             returning id",
        )
        .bind(context.site_id.into_uuid())
        .bind(run_id.into_uuid())
        .bind(input.flow_id.into_uuid())
        .bind(input.trigger.as_str())
        .bind(input.source_key.as_deref())
        .bind(&input.event)
        .bind(serde_json::to_value(&definition).map_err(|_| MaviError::Internal)?)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if inserted.is_none() {
            let existing: Option<Uuid> = sqlx::query_scalar(
                "select id from automation_runs where flow_id = $1 and source_key = $2",
            )
            .bind(input.flow_id.into_uuid())
            .bind(input.source_key.as_deref())
            .fetch_optional(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            if let Some(existing) = existing {
                return self.get_run(tx, FlowRunId::from_uuid(existing)).await;
            }
            return Err(MaviError::Internal);
        }
        enqueue_step(tx, context, jobs, run_id, 0, None).await?;
        audit(
            tx,
            context,
            "automation.run.started",
            "AutomationRun",
            run_id.into_uuid(),
            json!({"flow_id": input.flow_id, "trigger": input.trigger}),
        )
        .await?;
        self.get_run(tx, run_id).await
    }

    /// Persist one executor result and enqueue the next step atomically.
    #[allow(clippy::too_many_lines)]
    pub async fn record_step(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        jobs: &JobsService,
        input: &RecordStep,
    ) -> Result<FlowRun> {
        let row = sqlx::query(
            "select flow_id, definition, state from automation_runs
               where id = $1 for update",
        )
        .bind(input.run_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: "automation_run",
        })?;
        let state = RunState::parse(
            &row.try_get::<String, _>("state")
                .map_err(|_| MaviError::Internal)?,
        )?;
        if input.attempt < 1 {
            return Err(MaviError::validation("automation_attempt_invalid"));
        }
        if matches!(state, RunState::Succeeded | RunState::Failed) {
            // A worker may crash after recording the result and before
            // acknowledging its queue lease. Replaying that step is safe and
            // should not turn an already terminal run into a dead letter.
            return self.get_run(tx, input.run_id).await;
        }
        let definition: Vec<FlowStepInput> = serde_json::from_value(
            row.try_get::<Value, _>("definition")
                .map_err(|_| MaviError::Internal)?,
        )
        .map_err(|_| MaviError::Internal)?;
        let step = definition
            .get(
                usize::try_from(input.position)
                    .map_err(|_| MaviError::validation("flow_position_invalid"))?,
            )
            .ok_or_else(|| MaviError::validation("flow_position_invalid"))?;
        let finished_at = Utc::now();
        let step_id = FlowRunStepId::new();
        sqlx::query(
            "insert into automation_run_steps
                (site_id, id, run_id, position, attempt, kind, outcome, detail, error, started_at, finished_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             on conflict (site_id, run_id, position, attempt) do update
                set outcome = excluded.outcome, detail = excluded.detail,
                    error = excluded.error, finished_at = excluded.finished_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(step_id.into_uuid())
        .bind(input.run_id.into_uuid())
        .bind(input.position)
        .bind(input.attempt)
        .bind(step.kind.as_str())
        .bind(input.outcome.as_str())
        .bind(&input.detail)
        .bind(input.error.as_deref().map(|error| error.chars().take(4000).collect::<String>()))
        .bind(finished_at)
        .bind(finished_at)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        match input.outcome {
            StepOutcome::Failed => {
                sqlx::query(
                    "update automation_runs set state = 'failed', last_error = $2,
                            finished_at = now(), updated_at = now()
                       where id = $1",
                )
                .bind(input.run_id.into_uuid())
                .bind(input.error.as_deref())
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
            }
            StepOutcome::Succeeded | StepOutcome::Waiting => {
                let next_position = input.position.saturating_add(1);
                if next_position >= i32::try_from(definition.len()).unwrap_or(i32::MAX) {
                    sqlx::query(
                        "update automation_runs set state = 'succeeded', current_position = $2,
                                finished_at = now(), updated_at = now()
                           where id = $1",
                    )
                    .bind(input.run_id.into_uuid())
                    .bind(next_position)
                    .execute(tx.conn())
                    .await
                    .map_err(|_| MaviError::Internal)?;
                } else {
                    sqlx::query(
                        "update automation_runs set state = $2, current_position = $3,
                                last_error = null, updated_at = now()
                           where id = $1",
                    )
                    .bind(input.run_id.into_uuid())
                    .bind(if input.outcome == StepOutcome::Waiting {
                        RunState::Waiting.as_str()
                    } else {
                        RunState::Running.as_str()
                    })
                    .bind(next_position)
                    .execute(tx.conn())
                    .await
                    .map_err(|_| MaviError::Internal)?;
                    enqueue_step(
                        tx,
                        context,
                        jobs,
                        input.run_id,
                        next_position,
                        input.next_at,
                    )
                    .await?;
                }
            }
        }
        audit(
            tx,
            context,
            "automation.run.step_recorded",
            "AutomationRun",
            input.run_id.into_uuid(),
            json!({"position": input.position, "outcome": input.outcome}),
        )
        .await?;
        self.get_run(tx, input.run_id).await
    }

    pub async fn list_runs(
        &self,
        tx: &mut SiteTx,
        flow_id: FlowId,
        filter: &RunListFilter,
    ) -> Result<Page<FlowRun>> {
        // Resolve the parent first so a missing flow is not indistinguishable
        // from an empty run list.
        self.get(tx, flow_id).await?;
        let after = filter
            .page
            .after
            .as_ref()
            .map(decode_run_cursor)
            .transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query =
            QueryBuilder::<Postgres>::new("select id from automation_runs where flow_id = ");
        query.push_bind(flow_id.into_uuid());
        if let Some(state) = filter.state {
            query.push(" and state = ").push_bind(state.as_str());
        }
        if let Some(after) = after {
            query
                .push(" and (started_at, id) < (")
                .push_bind(after.started_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        query
            .push(" order by started_at desc, id desc limit ")
            .push_bind(limit + 1);
        let ids: Vec<Uuid> = query
            .build_query_scalar()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = Vec::with_capacity(ids.len());
        for id in ids {
            items.push(self.get_run(tx, FlowRunId::from_uuid(id)).await?);
        }
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let last = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_run_cursor(last.started_at, last.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn get_run(&self, tx: &mut SiteTx, id: FlowRunId) -> Result<FlowRun> {
        let row = sqlx::query(
            "select id, flow_id, trigger, event, definition, state, current_position,
                    retry_count, last_error, started_at, finished_at
               from automation_runs where id = $1",
        )
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: "automation_run",
        })?;
        let steps = sqlx::query(
            "select id, position, attempt, kind, outcome, detail, error, started_at, finished_at
               from automation_run_steps where run_id = $1 order by position, attempt",
        )
        .bind(id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(FlowRunStep {
                id: FlowRunStepId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
                position: row.try_get("position").map_err(|_| MaviError::Internal)?,
                attempt: row.try_get("attempt").map_err(|_| MaviError::Internal)?,
                kind: parse_step_kind(
                    &row.try_get::<String, _>("kind")
                        .map_err(|_| MaviError::Internal)?,
                )?,
                outcome: row.try_get("outcome").map_err(|_| MaviError::Internal)?,
                detail: row.try_get("detail").map_err(|_| MaviError::Internal)?,
                error: row.try_get("error").map_err(|_| MaviError::Internal)?,
                started_at: row.try_get("started_at").map_err(|_| MaviError::Internal)?,
                finished_at: row
                    .try_get("finished_at")
                    .map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        Ok(FlowRun {
            id,
            flow_id: FlowId::from_uuid(row.try_get("flow_id").map_err(|_| MaviError::Internal)?),
            trigger: parse_trigger(
                &row.try_get::<String, _>("trigger")
                    .map_err(|_| MaviError::Internal)?,
            )?,
            event: row.try_get("event").map_err(|_| MaviError::Internal)?,
            definition: serde_json::from_value(
                row.try_get::<Value, _>("definition")
                    .map_err(|_| MaviError::Internal)?,
            )
            .map_err(|_| MaviError::Internal)?,
            state: RunState::parse(
                &row.try_get::<String, _>("state")
                    .map_err(|_| MaviError::Internal)?,
            )?,
            current_position: row
                .try_get("current_position")
                .map_err(|_| MaviError::Internal)?,
            retry_count: row
                .try_get("retry_count")
                .map_err(|_| MaviError::Internal)?,
            last_error: row.try_get("last_error").map_err(|_| MaviError::Internal)?,
            steps,
            started_at: row.try_get("started_at").map_err(|_| MaviError::Internal)?,
            finished_at: row
                .try_get("finished_at")
                .map_err(|_| MaviError::Internal)?,
        })
    }
}

async fn insert_steps(
    tx: &mut SiteTx,
    context: &SiteContext,
    flow_id: FlowId,
    steps: &[FlowStepInput],
) -> Result<()> {
    for (position, step) in steps.iter().enumerate() {
        sqlx::query(
            "insert into automation_flow_steps
                (site_id, id, flow_id, position, kind, config)
             values ($1, $2, $3, $4, $5, $6)",
        )
        .bind(context.site_id.into_uuid())
        .bind(Uuid::now_v7())
        .bind(flow_id.into_uuid())
        .bind(i32::try_from(position).map_err(|_| MaviError::validation("flow_too_many_steps"))?)
        .bind(step.kind.as_str())
        .bind(&step.config)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    }
    Ok(())
}

async fn read_steps(tx: &mut SiteTx, flow_id: FlowId) -> Result<Vec<FlowStep>> {
    sqlx::query(
        "select id, position, kind, config from automation_flow_steps
           where flow_id = $1 order by position",
    )
    .bind(flow_id.into_uuid())
    .fetch_all(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?
    .iter()
    .map(|row| {
        Ok(FlowStep {
            id: FlowStepId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
            position: row.try_get("position").map_err(|_| MaviError::Internal)?,
            kind: parse_step_kind(
                &row.try_get::<String, _>("kind")
                    .map_err(|_| MaviError::Internal)?,
            )?,
            config: row.try_get("config").map_err(|_| MaviError::Internal)?,
        })
    })
    .collect()
}

async fn enqueue_step(
    tx: &mut SiteTx,
    context: &SiteContext,
    jobs: &JobsService,
    run_id: FlowRunId,
    position: i32,
    run_at: Option<DateTime<Utc>>,
) -> Result<JobId> {
    let payload = json!({"run_id": run_id, "position": position});
    let key = format!("flow-step:{run_id}:{position}");
    jobs.enqueue(
        tx,
        context,
        FLOW_STEP_KIND.name,
        &payload,
        run_at,
        Some(&key),
    )
    .await
}

fn validate_name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_FLOW_NAME {
        return Err(MaviError::validation("flow_name_invalid"));
    }
    Ok(value.to_owned())
}

fn validate_steps(steps: &[FlowStepInput]) -> Result<()> {
    if steps.is_empty() || steps.len() > MAX_FLOW_STEPS {
        return Err(MaviError::validation("flow_step_count_invalid"));
    }
    for step in steps {
        let object = step
            .config
            .as_object()
            .ok_or_else(|| MaviError::validation("flow_step_config_invalid"))?;
        match step.kind {
            StepKind::SendMail => {
                validate_config_keys(object, &["template_id"])?;
                require_uuid(object, "template_id", "flow_mail_template_required")?;
            }
            StepKind::AddToMailList => {
                validate_config_keys(object, &["list_id"])?;
                require_uuid(object, "list_id", "flow_mail_list_required")?;
            }
            StepKind::Wait => {
                validate_config_keys(object, &["seconds"])?;
                let seconds = object
                    .get("seconds")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| MaviError::validation("flow_wait_seconds_required"))?;
                if !(1..=MAX_WAIT_SECONDS).contains(&seconds) {
                    return Err(MaviError::validation("flow_wait_seconds_invalid"));
                }
            }
            StepKind::Webhook => {
                validate_config_keys(object, &["url"])?;
                let url = object
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| MaviError::validation("flow_webhook_url_required"))?;
                validate_webhook_url(url)?;
            }
        }
    }
    Ok(())
}

fn validate_config_keys(object: &serde_json::Map<String, Value>, allowed: &[&str]) -> Result<()> {
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(MaviError::validation("flow_step_config_unknown"));
    }
    Ok(())
}

fn require_uuid(
    object: &serde_json::Map<String, Value>,
    key: &str,
    code: &'static str,
) -> Result<()> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .ok_or(MaviError::Validation {
            code: code.to_owned(),
            field: Some(key.to_owned()),
        })?;
    Uuid::parse_str(value).map_err(|_| MaviError::validation_field(code, key))?;
    Ok(())
}

fn validate_webhook_url(value: &str) -> Result<()> {
    let value = value.trim();
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .ok_or_else(|| MaviError::validation("flow_webhook_url_invalid"))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') || authority.contains(' ') {
        return Err(MaviError::validation("flow_webhook_url_invalid"));
    }
    let host = authority
        .strip_prefix('[')
        .and_then(|value| value.split(']').next())
        .unwrap_or_else(|| authority.split(':').next().unwrap_or_default())
        .to_ascii_lowercase();
    if host.is_empty()
        || [
            "localhost",
            "localhost.localdomain",
            "metadata.google.internal",
        ]
        .contains(&host.as_str())
    {
        return Err(MaviError::validation("flow_webhook_private_address"));
    }
    if let Ok(ip) = host.parse::<IpAddr>()
        && private_ip(ip)
    {
        return Err(MaviError::validation("flow_webhook_private_address"));
    }
    Ok(())
}

fn private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_unspecified()
                || (ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1]))
        }
        IpAddr::V6(ip) => {
            if ip.is_loopback() || ip.is_unspecified() {
                return true;
            }
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return private_ip(IpAddr::V4(mapped));
            }
            let first = ip.segments()[0];
            (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
}

fn validate_event(event: &Value) -> Result<()> {
    if !event.is_object() {
        return Err(MaviError::validation("flow_event_must_be_object"));
    }
    Ok(())
}

fn parse_trigger(value: &str) -> Result<Trigger> {
    Trigger::ALL
        .into_iter()
        .find(|trigger| trigger.as_str() == value)
        .ok_or(MaviError::Internal)
}

fn parse_step_kind(value: &str) -> Result<StepKind> {
    [
        StepKind::SendMail,
        StepKind::Webhook,
        StepKind::Wait,
        StepKind::AddToMailList,
    ]
    .into_iter()
    .find(|kind| kind.as_str() == value)
    .ok_or(MaviError::Internal)
}

impl StepOutcome {
    #[must_use]
    const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Waiting => "waiting",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct FlowCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes =
        serde_json::to_vec(&FlowCursor { created_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor(cursor: &Cursor) -> Result<FlowCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RunCursor {
    started_at: DateTime<Utc>,
    id: Uuid,
}

fn encode_run_cursor(started_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes =
        serde_json::to_vec(&RunCursor { started_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_run_cursor(cursor: &Cursor) -> Result<RunCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

async fn audit(
    tx: &mut SiteTx,
    context: &SiteContext,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
    payload: Value,
) -> Result<()> {
    AuditService
        .record(
            tx,
            context,
            &AuditEntry {
                action: action.to_owned(),
                resource_type: resource_type.to_owned(),
                resource_id: Some(resource_id),
                payload,
            },
        )
        .await
}

#[must_use]
pub fn trigger_descriptions() -> Vec<TriggerDescription> {
    Trigger::ALL
        .into_iter()
        .map(|trigger| TriggerDescription {
            trigger,
            emitted_by: match trigger {
                Trigger::ContentPublished => "mavi-content",
                Trigger::FormSubmitted => "mavi-forms",
                Trigger::OrderPaid | Trigger::OrderSent => "mavi-shop",
                Trigger::CourseEnrollmentCreated | Trigger::CourseLessonCompleted => "mavi-courses",
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_steps_are_validated_before_a_flow_is_written() {
        let invalid = [FlowStepInput {
            kind: StepKind::Wait,
            config: json!({"seconds": 0}),
        }];
        assert!(validate_steps(&invalid).is_err());

        let valid = [FlowStepInput {
            kind: StepKind::Wait,
            config: json!({"seconds": 60}),
        }];
        assert!(validate_steps(&valid).is_ok());
    }

    #[test]
    fn webhook_validation_rejects_private_targets() {
        assert!(validate_webhook_url("http://127.0.0.1:8080").is_err());
        assert!(validate_webhook_url("https://example.test/hook").is_ok());
        assert!(validate_webhook_url("https://user:password@example.test").is_err());
    }

    #[test]
    fn flow_contract_is_cursor_only() {
        let api = api();
        let filter = shapes()
            .into_iter()
            .find(|shape| shape.name == "FlowListFilter")
            .expect("filter");
        let properties = filter.schema["properties"].as_object().expect("properties");
        assert!(properties.contains_key("after"));
        assert!(properties.contains_key("limit"));
        assert!(!properties.contains_key("page"));
        assert!(!properties.contains_key("offset"));
        api.validate().expect("valid API");
    }

    #[test]
    fn run_step_is_not_allowed_past_the_snapshot() {
        let position = 32_i32;
        assert!(usize::try_from(position).expect("position") >= MAX_FLOW_STEPS);
    }
}
