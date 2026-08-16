//! Reading and writing what a site does by itself.
//!
//! A flow is written whole: its trigger, its steps, and every step's settings
//! checked before a row exists. That is the point of the crate — a flow that
//! cannot run is a flow that is never written down, rather than one that fails
//! once per event for as long as it exists.

use chrono::{DateTime, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::page::{Page, Query};
use mavi_core::say::Say;
use mavi_db::{Tx, Walk};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::run::State;
use crate::step::{Does, Step, Trigger, all_of_them};
use crate::{BY_RECENT, RUNS_BY_START};

pub const THERE_IS_NO_FLOW_LIKE_THAT: &str = "there_is_no_flow_like_that";
pub const THERE_IS_NO_RUN_LIKE_THAT: &str = "there_is_no_run_like_that";

/// One flow, whole.
#[derive(Clone, Debug, Serialize)]
pub struct Flow {
    pub id: Uuid,
    pub name: String,
    pub trigger: String,
    pub on: bool,
    pub steps: Vec<Told>,
    pub created_at: DateTime<Utc>,
}

/// One step, as it is read back.
#[derive(Clone, Debug, Serialize)]
pub struct Told {
    pub does: String,
    pub told: serde_json::Value,
    pub place: i32,
}

/// One journey through a flow.
#[derive(Clone, Debug, Serialize)]
pub struct Run {
    pub id: Uuid,
    pub flow_id: Uuid,
    pub state: String,
    pub about: serde_json::Value,
    pub at_step: i32,
    pub went_wrong: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// What this site does by itself.
pub async fn list(tx: &mut Tx, query: &Query) -> Result<Page<Flow>> {
    let walk = Walk::new(BY_RECENT, query.after(BY_RECENT)?);
    let mut wheres = vec!["deleted_at is null".to_owned()];

    let cursor = walk.after(1);
    if let Some((sql, _)) = &cursor {
        wheres.push(sql.clone());
    }

    let sql = format!(
        "select id, name, trigger, on_, created_at from flows
          where {} order by {} limit {}",
        wheres.join(" and "),
        walk.order(),
        query.fetch(),
    );

    let mut asking = sqlx::query(&sql);

    if let Some((_, values)) = cursor {
        for value in values {
            asking = asking.bind(value);
        }
    }

    let rows = asking.fetch_all(tx.conn()).await.map_err(Error::internal)?;

    let mut flows = Vec::with_capacity(rows.len());

    for row in &rows {
        let id: Uuid = row.try_get("id").map_err(Error::internal)?;

        flows.push(Flow {
            id,
            name: row.try_get("name").map_err(Error::internal)?,
            trigger: row.try_get("trigger").map_err(Error::internal)?,
            on: row.try_get("on_").map_err(Error::internal)?,
            steps: steps(tx, id).await?,
            created_at: row.try_get("created_at").map_err(Error::internal)?,
        });
    }

    Page::build(query, BY_RECENT, flows, |flow| {
        vec![flow.created_at.to_rfc3339(), flow.id.to_string()]
    })
}

async fn steps(tx: &mut Tx, flow: Uuid) -> Result<Vec<Told>> {
    let rows = sqlx::query("select does, told, place from steps where flow_id = $1 order by place")
        .bind(flow)
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?;

    rows.iter()
        .map(|row| {
            Ok(Told {
                does: row.try_get("does").map_err(Error::internal)?,
                told: row.try_get("told").map_err(Error::internal)?,
                place: row.try_get("place").map_err(Error::internal)?,
            })
        })
        .collect()
}

/// What arranging one asks for.
///
/// Serialised as well as read, so the test beside the description can hold
/// what it says it takes against what it takes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewFlow {
    pub name: String,
    pub trigger: String,
    pub steps: Vec<NewStep>,
}

/// One step, as it arrives.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewStep {
    pub does: String,
    #[serde(default)]
    pub told: serde_json::Value,
}

/// Arranges one.
///
/// Every step is checked here, so a flow in the table is a flow that can run.
/// The trigger is checked too: waiting for something nothing emits is a flow
/// that never runs and nobody is told about.
pub async fn make(tx: &mut Tx, new: &NewFlow) -> Result<Flow> {
    let trigger = Trigger::parse(&new.trigger)?;

    let checked = new
        .steps
        .iter()
        .map(|step| {
            let does = Does::parse(&step.does)?;

            Step::checked(does, &step.told)
        })
        .collect::<Result<Vec<_>>>()?;

    let checked = all_of_them(checked)?;

    let id = Uuid::now_v7();

    sqlx::query("insert into flows (id, name, trigger) values ($1, $2, $3)")
        .bind(id)
        .bind(new.name.trim())
        .bind(trigger.as_str())
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    for (at, step) in checked.iter().enumerate() {
        sqlx::query(
            "insert into steps (id, flow_id, does, told, place) values ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::now_v7())
        .bind(id)
        .bind(step.does.as_str())
        .bind(&step.told)
        .bind(i32::try_from(at).unwrap_or(i32::MAX))
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;
    }

    read(tx, id).await
}

/// One flow.
pub async fn read(tx: &mut Tx, id: Uuid) -> Result<Flow> {
    let row = sqlx::query(
        "select id, name, trigger, on_, created_at from flows
          where id = $1 and deleted_at is null",
    )
    .bind(id)
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?
    .ok_or_else(|| Error::not_found(Say::of(THERE_IS_NO_FLOW_LIKE_THAT)))?;

    Ok(Flow {
        id,
        name: row.try_get("name").map_err(Error::internal)?,
        trigger: row.try_get("trigger").map_err(Error::internal)?,
        on: row.try_get("on_").map_err(Error::internal)?,
        steps: steps(tx, id).await?,
        created_at: row.try_get("created_at").map_err(Error::internal)?,
    })
}

/// What may be changed about a flow.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FlowChanges {
    pub name: Option<String>,
    pub on: Option<bool>,
    /// The whole list, replaced. A flow's steps are one thing rather than a
    /// collection to add to: what somebody is editing is the order and the
    /// settings together.
    pub steps: Option<Vec<NewStep>>,
}

/// Changes what a flow does, or turns it on or off.
pub async fn change(tx: &mut Tx, id: Uuid, changes: &FlowChanges) -> Result<Flow> {
    let touched = sqlx::query(
        "update flows set name = coalesce($2, name), on_ = coalesce($3, on_), updated_at = now()
          where id = $1 and deleted_at is null",
    )
    .bind(id)
    .bind(changes.name.as_deref())
    .bind(changes.on)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    if touched.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(THERE_IS_NO_FLOW_LIKE_THAT)));
    }

    if let Some(steps) = &changes.steps {
        let checked = steps
            .iter()
            .map(|step| {
                let does = Does::parse(&step.does)?;

                Step::checked(does, &step.told)
            })
            .collect::<Result<Vec<_>>>()?;

        let checked = all_of_them(checked)?;

        // Replaced as one thing, in the caller's transaction, so no run ever
        // reads half of an old flow and half of a new one.
        sqlx::query("delete from steps where flow_id = $1")
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(Error::internal)?;

        for (at, step) in checked.iter().enumerate() {
            sqlx::query(
                "insert into steps (id, flow_id, does, told, place) values ($1, $2, $3, $4, $5)",
            )
            .bind(Uuid::now_v7())
            .bind(id)
            .bind(step.does.as_str())
            .bind(&step.told)
            .bind(i32::try_from(at).unwrap_or(i32::MAX))
            .execute(tx.conn())
            .await
            .map_err(Error::internal)?;
        }
    }

    read(tx, id).await
}

/// Stops arranging it. Runs that already happened stay.
pub async fn remove(tx: &mut Tx, id: Uuid) -> Result<()> {
    let gone = sqlx::query(
        "update flows set deleted_at = now(), on_ = false
          where id = $1 and deleted_at is null",
    )
    .bind(id)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    if gone.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(THERE_IS_NO_FLOW_LIKE_THAT)));
    }

    Ok(())
}

/// What this flow has done, most recent first.
pub async fn runs(
    tx: &mut Tx,
    flow: Uuid,
    state: Option<&str>,
    query: &Query,
) -> Result<Page<Run>> {
    let walk = Walk::new(RUNS_BY_START, query.after(RUNS_BY_START)?);
    let mut wheres = vec!["flow_id = $1".to_owned()];
    let mut binds: Vec<String> = Vec::new();

    if let Some(state) = state {
        binds.push(state.to_owned());
        wheres.push(format!("state = ${}", binds.len() + 1));
    }

    let cursor = walk.after(binds.len() + 2);
    if let Some((sql, _)) = &cursor {
        wheres.push(sql.clone());
    }

    let sql = format!(
        "select id, flow_id, state, about, at_step, went_wrong, started_at, finished_at
           from runs where {} order by {} limit {}",
        wheres.join(" and "),
        walk.order(),
        query.fetch(),
    );

    let mut asking = sqlx::query(&sql).bind(flow);

    for bind in binds {
        asking = asking.bind(bind);
    }

    if let Some((_, values)) = cursor {
        for value in values {
            asking = asking.bind(value);
        }
    }

    let rows = asking
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?
        .iter()
        .map(a_run)
        .collect::<Result<Vec<_>>>()?;

    Page::build(query, RUNS_BY_START, rows, |run| {
        vec![run.started_at.to_rfc3339(), run.id.to_string()]
    })
}

fn a_run(row: &sqlx::postgres::PgRow) -> Result<Run> {
    Ok(Run {
        id: row.try_get("id").map_err(Error::internal)?,
        flow_id: row.try_get("flow_id").map_err(Error::internal)?,
        state: row.try_get("state").map_err(Error::internal)?,
        about: row.try_get("about").map_err(Error::internal)?,
        at_step: row.try_get("at_step").map_err(Error::internal)?,
        went_wrong: row.try_get("went_wrong").map_err(Error::internal)?,
        started_at: row.try_get("started_at").map_err(Error::internal)?,
        finished_at: row.try_get("finished_at").map_err(Error::internal)?,
    })
}

/// One run: what set it off, and where it got to.
pub async fn a_run_of_it(tx: &mut Tx, id: Uuid) -> Result<Run> {
    let row = sqlx::query(
        "select id, flow_id, state, about, at_step, went_wrong, started_at, finished_at
           from runs where id = $1",
    )
    .bind(id)
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?
    .ok_or_else(|| Error::not_found(Say::of(THERE_IS_NO_RUN_LIKE_THAT)))?;

    a_run(&row)
}

/// What a flow would do about something made up.
///
/// Nothing leaves the machine: no letter, no call, no row. Whoever is
/// arranging a flow should be able to see what it would do without writing to
/// a customer to find out.
#[derive(Clone, Debug, Serialize)]
pub struct WouldDo {
    pub does: String,
    pub told: serde_json::Value,
    /// What the step would be working with, once the values from the thing
    /// that set it off are put in.
    pub about: serde_json::Value,
}

/// Runs a flow against something made up, and sends nothing.
pub async fn would_do(tx: &mut Tx, id: Uuid, about: &serde_json::Value) -> Result<Vec<WouldDo>> {
    let flow = read(tx, id).await?;

    Ok(flow
        .steps
        .into_iter()
        .map(|step| WouldDo {
            does: step.does,
            told: step.told,
            about: about.clone(),
        })
        .collect())
}

/// Every flow waiting for this to happen.
///
/// What the runner asks when something does. Only the ones that are on: a flow
/// somebody is still writing does not run because they saved it.
pub async fn waiting_for(tx: &mut Tx, trigger: Trigger) -> Result<Vec<Uuid>> {
    sqlx::query_scalar("select id from flows where trigger = $1 and on_ and deleted_at is null")
        .bind(trigger.as_str())
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)
}

/// Starts a run, holding what set it off as it was at the time.
pub async fn begin(tx: &mut Tx, flow: Uuid, about: &serde_json::Value) -> Result<Uuid> {
    let id = Uuid::now_v7();

    sqlx::query("insert into runs (id, flow_id, about) values ($1, $2, $3)")
        .bind(id)
        .bind(flow)
        .bind(about)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    Ok(id)
}

/// Writes down where a run has got to.
pub async fn got_to(
    tx: &mut Tx,
    run: Uuid,
    state: State,
    at_step: i32,
    went_wrong: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "update runs
            set state = $2,
                at_step = $3,
                went_wrong = coalesce($4, went_wrong),
                finished_at = case when $2 in ('done', 'stuck') then now() end
          where id = $1",
    )
    .bind(run)
    .bind(state.as_str())
    .bind(at_step)
    .bind(went_wrong)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(())
}
