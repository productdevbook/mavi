use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_core::{MaviError, Result, SiteContext, SiteId};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use super::{FlowService, FlowStepInput, RunState, StepKind, StepOutcome, Trigger, validate_steps};

pub const FLOWS_RELOCATION_FORMAT: &str = "mavi.flows.relocation";
pub const FLOWS_RELOCATION_VERSION: u16 = 1;
pub const MAX_FLOWS_RELOCATION_RECORDS: usize = 100_000;
pub const MAX_FLOWS_RELOCATION_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowsRelocation {
    pub format: String,
    pub version: u16,
    pub source_site_id: SiteId,
    pub flows: Vec<FlowRelocation>,
    pub steps: Vec<FlowStepRelocation>,
    pub runs: Vec<FlowRunRelocation>,
    pub run_steps: Vec<FlowRunStepRelocation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowRelocation {
    pub id: Uuid,
    pub name: String,
    pub trigger: Trigger,
    pub enabled: bool,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowStepRelocation {
    pub id: Uuid,
    pub flow_id: Uuid,
    pub position: i32,
    pub kind: StepKind,
    pub config: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowRunRelocation {
    pub id: Uuid,
    pub flow_id: Uuid,
    pub trigger: Trigger,
    pub source_key: Option<String>,
    pub event: Value,
    pub definition: Vec<FlowStepInput>,
    pub state: RunState,
    pub current_position: i32,
    pub retry_count: i32,
    pub last_error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowRunStepRelocation {
    pub id: Uuid,
    pub run_id: Uuid,
    pub position: i32,
    pub attempt: i32,
    pub kind: StepKind,
    pub outcome: StepOutcome,
    pub detail: Value,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
}

impl FlowsRelocation {
    #[must_use]
    pub fn empty(source_site_id: SiteId) -> Self {
        Self {
            format: FLOWS_RELOCATION_FORMAT.to_owned(),
            version: FLOWS_RELOCATION_VERSION,
            source_site_id,
            flows: Vec::new(),
            steps: Vec::new(),
            runs: Vec::new(),
            run_steps: Vec::new(),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn validate_for_relocation(&self, target_site: SiteId) -> Result<()> {
        if self.format != FLOWS_RELOCATION_FORMAT {
            return Err(MaviError::validation("flows_relocation_format_invalid"));
        }
        if self.version != FLOWS_RELOCATION_VERSION {
            return Err(MaviError::validation(
                "flows_relocation_version_unsupported",
            ));
        }
        if self.source_site_id != target_site || self.source_site_id.into_uuid().is_nil() {
            return Err(MaviError::conflict("flows_relocation_site_mismatch"));
        }
        let sections = [
            self.flows.len(),
            self.steps.len(),
            self.runs.len(),
            self.run_steps.len(),
        ];
        let total = sections
            .iter()
            .try_fold(0usize, |total, count| total.checked_add(*count))
            .ok_or_else(|| MaviError::validation("flows_relocation_count_overflow"))?;
        if total > MAX_FLOWS_RELOCATION_RECORDS
            || sections
                .iter()
                .any(|count| *count > MAX_FLOWS_RELOCATION_RECORDS)
        {
            return Err(MaviError::validation("flows_relocation_counts_invalid"));
        }

        let mut flow_ids = BTreeSet::new();
        let mut active_names = BTreeSet::new();
        for flow in &self.flows {
            if flow.id.is_nil()
                || !flow_ids.insert(flow.id)
                || flow.name.trim().is_empty()
                || flow.name.chars().count() > 200
                || flow.version < 1
                || (flow.deleted_at.is_none()
                    && !active_names.insert(flow.name.to_ascii_lowercase()))
            {
                return Err(MaviError::validation("flows_relocation_flow_invalid"));
            }
        }

        let mut step_ids = BTreeSet::new();
        let mut step_positions = BTreeSet::new();
        for step in &self.steps {
            if step.id.is_nil()
                || !step_ids.insert(step.id)
                || !flow_ids.contains(&step.flow_id)
                || step.position < 0
                || !step_positions.insert((step.flow_id, step.position))
            {
                return Err(MaviError::validation("flows_relocation_step_invalid"));
            }
            validate_steps(&[FlowStepInput {
                kind: step.kind,
                config: step.config.clone(),
            }])?;
        }

        let mut run_ids = BTreeSet::new();
        let mut source_keys = BTreeSet::new();
        for run in &self.runs {
            let finished_state = matches!(run.state, RunState::Succeeded | RunState::Failed);
            if run.id.is_nil()
                || !run_ids.insert(run.id)
                || !flow_ids.contains(&run.flow_id)
                || !run.event.is_object()
                || run.definition.is_empty()
                || run.definition.len() > 32
                || validate_steps(&run.definition).is_err()
                || run.current_position < 0
                || usize::try_from(run.current_position)
                    .is_ok_and(|position| position > run.definition.len())
                || run.retry_count < 0
                || run
                    .last_error
                    .as_deref()
                    .is_some_and(|error| error.chars().count() > 4_000)
                || run.source_key.as_deref().is_some_and(|key| {
                    key.is_empty()
                        || key.chars().count() > 160
                        || !source_keys.insert((run.flow_id, key.to_owned()))
                })
                || (finished_state != run.finished_at.is_some())
            {
                return Err(MaviError::validation("flows_relocation_run_invalid"));
            }
        }

        let mut run_step_ids = BTreeSet::new();
        let mut run_step_keys = BTreeSet::new();
        for step in &self.run_steps {
            let Some(run) = self.runs.iter().find(|run| run.id == step.run_id) else {
                return Err(MaviError::validation("flows_relocation_run_step_invalid"));
            };
            let Some(definition_step) = run
                .definition
                .get(usize::try_from(step.position).unwrap_or(usize::MAX))
            else {
                return Err(MaviError::validation("flows_relocation_run_step_invalid"));
            };
            if step.id.is_nil()
                || !run_step_ids.insert(step.id)
                || step.position < 0
                || step.attempt < 1
                || definition_step.kind != step.kind
                || !step.detail.is_object()
                || step
                    .error
                    .as_deref()
                    .is_some_and(|error| error.chars().count() > 4_000)
                || !run_step_keys.insert((step.run_id, step.position, step.attempt))
            {
                return Err(MaviError::validation("flows_relocation_run_step_invalid"));
            }
        }

        if serde_json::to_vec(self)
            .map_err(|_| MaviError::Internal)?
            .len()
            > MAX_FLOWS_RELOCATION_BYTES
        {
            return Err(MaviError::validation("flows_relocation_too_large"));
        }
        Ok(())
    }

    pub fn record_count(&self) -> Result<i64> {
        let count = self
            .flows
            .len()
            .checked_add(self.steps.len())
            .and_then(|value| value.checked_add(self.runs.len()))
            .and_then(|value| value.checked_add(self.run_steps.len()))
            .ok_or_else(|| MaviError::validation("flows_relocation_count_overflow"))?;
        i64::try_from(count).map_err(|_| MaviError::validation("flows_relocation_count_overflow"))
    }
}

impl FlowService {
    #[allow(clippy::too_many_lines)]
    pub async fn export_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
    ) -> Result<FlowsRelocation> {
        let site_id = context.site_id.into_uuid();
        let flows = sqlx::query(
            "select id, name, trigger, enabled, version, created_at, updated_at, deleted_at
               from automation_flows where site_id = $1 order by created_at, id",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(FlowRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                trigger: parse_trigger(
                    &row.try_get::<String, _>("trigger")
                        .map_err(|_| MaviError::Internal)?,
                )?,
                enabled: row.try_get("enabled").map_err(|_| MaviError::Internal)?,
                version: row.try_get("version").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let steps = sqlx::query(
            "select id, flow_id, position, kind, config
               from automation_flow_steps where site_id = $1 order by flow_id, position, id",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(FlowStepRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                flow_id: row.try_get("flow_id").map_err(|_| MaviError::Internal)?,
                position: row.try_get("position").map_err(|_| MaviError::Internal)?,
                kind: parse_step_kind(
                    &row.try_get::<String, _>("kind")
                        .map_err(|_| MaviError::Internal)?,
                )?,
                config: row.try_get("config").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let runs = sqlx::query(
            "select id, flow_id, trigger, source_key, event, definition, state,
                    current_position, retry_count, last_error, started_at, updated_at, finished_at
               from automation_runs where site_id = $1 order by started_at, id",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(FlowRunRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                flow_id: row.try_get("flow_id").map_err(|_| MaviError::Internal)?,
                trigger: parse_trigger(
                    &row.try_get::<String, _>("trigger")
                        .map_err(|_| MaviError::Internal)?,
                )?,
                source_key: row.try_get("source_key").map_err(|_| MaviError::Internal)?,
                event: row.try_get("event").map_err(|_| MaviError::Internal)?,
                definition: serde_json::from_value(
                    row.try_get("definition").map_err(|_| MaviError::Internal)?,
                )
                .map_err(|_| MaviError::Internal)?,
                state: parse_run_state(
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
                started_at: row.try_get("started_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                finished_at: row
                    .try_get("finished_at")
                    .map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let run_steps = sqlx::query(
            "select id, run_id, position, attempt, kind, outcome, detail, error, started_at, finished_at
               from automation_run_steps where site_id = $1 order by run_id, position, attempt",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(FlowRunStepRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                run_id: row.try_get("run_id").map_err(|_| MaviError::Internal)?,
                position: row.try_get("position").map_err(|_| MaviError::Internal)?,
                attempt: row.try_get("attempt").map_err(|_| MaviError::Internal)?,
                kind: parse_step_kind(&row.try_get::<String, _>("kind").map_err(|_| MaviError::Internal)?)?,
                outcome: parse_step_outcome(&row.try_get::<String, _>("outcome").map_err(|_| MaviError::Internal)?)?,
                detail: row.try_get("detail").map_err(|_| MaviError::Internal)?,
                error: row.try_get("error").map_err(|_| MaviError::Internal)?,
                started_at: row.try_get("started_at").map_err(|_| MaviError::Internal)?,
                finished_at: row.try_get("finished_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let relocation = FlowsRelocation {
            format: FLOWS_RELOCATION_FORMAT.to_owned(),
            version: FLOWS_RELOCATION_VERSION,
            source_site_id: context.site_id,
            flows,
            steps,
            runs,
            run_steps,
        };
        relocation.validate_for_relocation(context.site_id)?;
        Ok(relocation)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn import_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        relocation: &FlowsRelocation,
    ) -> Result<()> {
        relocation.validate_for_relocation(context.site_id)?;
        let site_id = context.site_id.into_uuid();
        for table in [
            "automation_run_steps",
            "automation_runs",
            "automation_flow_steps",
            "automation_flows",
        ] {
            let statement = match table {
                "automation_run_steps" => "delete from automation_run_steps where site_id = $1",
                "automation_runs" => "delete from automation_runs where site_id = $1",
                "automation_flow_steps" => "delete from automation_flow_steps where site_id = $1",
                "automation_flows" => "delete from automation_flows where site_id = $1",
                _ => return Err(MaviError::Internal),
            };
            sqlx::query(statement)
                .bind(site_id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
        }
        for flow in &relocation.flows {
            sqlx::query(
                "insert into automation_flows
                    (site_id, id, name, trigger, enabled, version, created_at, updated_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(site_id)
            .bind(flow.id)
            .bind(&flow.name)
            .bind(flow.trigger.as_str())
            .bind(flow.enabled)
            .bind(flow.version)
            .bind(flow.created_at)
            .bind(flow.updated_at)
            .bind(flow.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        for step in &relocation.steps {
            sqlx::query(
                "insert into automation_flow_steps
                    (site_id, id, flow_id, position, kind, config)
                 values ($1, $2, $3, $4, $5, $6)",
            )
            .bind(site_id)
            .bind(step.id)
            .bind(step.flow_id)
            .bind(step.position)
            .bind(step.kind.as_str())
            .bind(&step.config)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        for run in &relocation.runs {
            sqlx::query(
                "insert into automation_runs
                    (site_id, id, flow_id, trigger, source_key, event, definition, state,
                     current_position, retry_count, last_error, started_at, updated_at, finished_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
            )
            .bind(site_id)
            .bind(run.id)
            .bind(run.flow_id)
            .bind(run.trigger.as_str())
            .bind(&run.source_key)
            .bind(&run.event)
            .bind(serde_json::to_value(&run.definition).map_err(|_| MaviError::Internal)?)
            .bind(run.state.as_str())
            .bind(run.current_position)
            .bind(run.retry_count)
            .bind(&run.last_error)
            .bind(run.started_at)
            .bind(run.updated_at)
            .bind(run.finished_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        for step in &relocation.run_steps {
            sqlx::query(
                "insert into automation_run_steps
                    (site_id, id, run_id, position, attempt, kind, outcome, detail, error, started_at, finished_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
            )
            .bind(site_id)
            .bind(step.id)
            .bind(step.run_id)
            .bind(step.position)
            .bind(step.attempt)
            .bind(step.kind.as_str())
            .bind(step.outcome.as_str())
            .bind(&step.detail)
            .bind(&step.error)
            .bind(step.started_at)
            .bind(step.finished_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "portable.flows.relocated".to_owned(),
                    resource_type: "FlowsSnapshot".to_owned(),
                    resource_id: None,
                    payload: serde_json::json!({
                        "flows": relocation.flows.len(),
                        "steps": relocation.steps.len(),
                        "runs": relocation.runs.len(),
                        "run_steps": relocation.run_steps.len(),
                        "active_runs_resumable": true,
                    }),
                },
            )
            .await
    }
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

fn parse_run_state(value: &str) -> Result<RunState> {
    match value {
        "running" => Ok(RunState::Running),
        "waiting" => Ok(RunState::Waiting),
        "succeeded" => Ok(RunState::Succeeded),
        "failed" => Ok(RunState::Failed),
        _ => Err(MaviError::Internal),
    }
}

fn parse_step_outcome(value: &str) -> Result<StepOutcome> {
    match value {
        "succeeded" => Ok(StepOutcome::Succeeded),
        "waiting" => Ok(StepOutcome::Waiting),
        "failed" => Ok(StepOutcome::Failed),
        _ => Err(MaviError::Internal),
    }
}
