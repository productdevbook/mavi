#![allow(clippy::wildcard_imports)]

use super::super::*;

pub(super) fn automation_routes() -> Router<HttpState> {
    Router::new()
        .merge(workflow_routes())
        .route("/api/v1/automation/triggers", get(list_automation_triggers))
        .route(
            "/api/v1/automation/flows",
            get(list_flows).post(create_flow),
        )
        .route(
            "/api/v1/automation/flows/{id}",
            get(read_flow).patch(update_flow).delete(delete_flow),
        )
        .route(
            "/api/v1/automation/flows/{id}/simulate",
            post(simulate_flow),
        )
        .route("/api/v1/automation/flows/{id}/runs", get(list_flow_runs))
        .route("/api/v1/automation/runs/{id}", get(read_flow_run))
}

pub(super) fn workflow_routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/workflows/runs", get(list_workflow_runs))
        .route("/api/v1/workflows/runs/{id}", get(read_workflow_run))
        .route(
            "/api/v1/workflows/runs/{id}/cancel",
            post(cancel_workflow_run),
        )
        .route(
            "/api/v1/workflows/runs/{id}/replay",
            post(replay_workflow_run),
        )
        .route(
            "/api/v1/workflows/runs/{id}/pause",
            post(pause_workflow_run),
        )
        .route(
            "/api/v1/workflows/runs/{id}/resume",
            post(resume_workflow_run),
        )
}

async fn list_workflow_runs(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<WorkflowRunListFilter>,
) -> Result<Json<Page<WorkflowRunRecord>>, HttpError> {
    require_workflow_permission(
        &state,
        &context,
        "workflow.view",
        "WorkflowRun",
        "workflow_runs",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let runs = state
        .workflows
        .list_runs(&mut transaction, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(runs))
}

async fn read_workflow_run(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunRecord>, HttpError> {
    require_workflow_permission(&state, &context, "workflow.view", "WorkflowRun", id.clone())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let run = state
        .workflows
        .get_run(&mut transaction, &id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(run))
}

async fn cancel_workflow_run(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunRecord>, HttpError> {
    require_workflow_permission(
        &state,
        &context,
        "workflow.control",
        "WorkflowRun",
        id.clone(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let existing = state
        .workflows
        .get_run(&mut transaction, &id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    require_hatchet_bridge_for_run(&state, &existing)?;
    let replacement_run_id = if let (Some(bridge), Some(hatchet_run_id)) = (
        state.hatchet_bridge.as_ref(),
        existing.hatchet_run_id.as_deref(),
    ) {
        bridge.cancel_run(hatchet_run_id).await.map_err(HttpError)?
    } else {
        None
    };
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    if existing
        .hatchet_run_id
        .as_deref()
        .is_some_and(|run_id| run_id.starts_with("schedule:"))
    {
        state
            .workflows
            .set_hatchet_run_id(&mut transaction, &id, replacement_run_id.as_deref())
            .await
            .map_err(HttpError)?;
    }
    let run = state
        .workflows
        .cancel(&mut transaction, &id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(run))
}

async fn replay_workflow_run(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunRecord>, HttpError> {
    require_workflow_permission(
        &state,
        &context,
        "workflow.control",
        "WorkflowRun",
        id.clone(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let existing = state
        .workflows
        .get_run(&mut transaction, &id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    if existing.status == "running" {
        return Err(HttpError(MaviError::conflict(
            "workflow_run_not_replayable",
        )));
    }
    require_hatchet_bridge_for_run(&state, &existing)?;
    let run = if let (Some(bridge), Some(hatchet_run_id)) = (
        state.hatchet_bridge.as_ref(),
        existing.hatchet_run_id.as_deref(),
    ) {
        let replacement_run_id = bridge.replay_run(hatchet_run_id).await.map_err(HttpError)?;
        let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
        let run = state
            .workflows
            .mark_replayed(&mut transaction, &id, replacement_run_id.as_deref())
            .await
            .map_err(HttpError)?;
        transaction.commit().await.map_err(HttpError)?;
        run
    } else {
        let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
        let run = state
            .workflows
            .replay(&mut transaction, &id)
            .await
            .map_err(HttpError)?;
        transaction.commit().await.map_err(HttpError)?;
        run
    };
    Ok(Json(run))
}

async fn pause_workflow_run(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunRecord>, HttpError> {
    require_workflow_permission(
        &state,
        &context,
        "workflow.control",
        "WorkflowRun",
        id.clone(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let existing = state
        .workflows
        .get_run(&mut transaction, &id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    require_hatchet_bridge_for_run(&state, &existing)?;
    let replacement_run_id = if let (Some(bridge), Some(hatchet_run_id)) = (
        state.hatchet_bridge.as_ref(),
        existing.hatchet_run_id.as_deref(),
    ) {
        bridge.cancel_run(hatchet_run_id).await.map_err(HttpError)?
    } else {
        None
    };
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    if existing
        .hatchet_run_id
        .as_deref()
        .is_some_and(|run_id| run_id.starts_with("schedule:"))
    {
        state
            .workflows
            .set_hatchet_run_id(&mut transaction, &id, replacement_run_id.as_deref())
            .await
            .map_err(HttpError)?;
    }
    let run = state
        .workflows
        .pause(&mut transaction, &id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(run))
}

async fn resume_workflow_run(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowRunRecord>, HttpError> {
    require_workflow_permission(
        &state,
        &context,
        "workflow.control",
        "WorkflowRun",
        id.clone(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let existing = state
        .workflows
        .get_run(&mut transaction, &id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    if existing.status != "paused" {
        return Err(HttpError(MaviError::conflict("workflow_run_not_resumable")));
    }
    require_hatchet_bridge_for_run(&state, &existing)?;
    let run = if let (Some(bridge), Some(hatchet_run_id)) = (
        state.hatchet_bridge.as_ref(),
        existing.hatchet_run_id.as_deref(),
    ) {
        let replacement_run_id = bridge.replay_run(hatchet_run_id).await.map_err(HttpError)?;
        let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
        let run = state
            .workflows
            .mark_replayed(&mut transaction, &id, replacement_run_id.as_deref())
            .await
            .map_err(HttpError)?;
        transaction.commit().await.map_err(HttpError)?;
        run
    } else {
        let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
        let run = state
            .workflows
            .resume(&mut transaction, &id)
            .await
            .map_err(HttpError)?;
        transaction.commit().await.map_err(HttpError)?;
        run
    };
    Ok(Json(run))
}

fn require_hatchet_bridge_for_run(
    state: &HttpState,
    run: &WorkflowRunRecord,
) -> Result<(), HttpError> {
    if run.hatchet_run_id.is_some() && state.hatchet_bridge.is_none() {
        // A local state transition cannot cancel, replay, or pause a run that
        // Hatchet already owns. Failing closed prevents the API projection
        // from claiming that delivery stopped while the remote run continues.
        return Err(HttpError(MaviError::Internal));
    }
    Ok(())
}

async fn list_automation_triggers(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
) -> Result<Json<Vec<TriggerDescription>>, HttpError> {
    require_automation_grant(
        &state,
        &context,
        Action::View,
        "AutomationTrigger",
        "trigger_collection",
    )?;
    Ok(Json(mavi_flows::trigger_descriptions()))
}

async fn list_flows(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<FlowListFilter>,
) -> Result<Json<Page<Flow>>, HttpError> {
    require_automation_grant(
        &state,
        &context,
        Action::View,
        "AutomationFlow",
        "flow_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .flows
        .list(&mut transaction, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn create_flow(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateFlow>,
) -> Result<(StatusCode, Json<Flow>), HttpError> {
    require_automation_grant(
        &state,
        &context,
        Action::Write,
        "AutomationFlow",
        "flow_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let flow = state
        .flows
        .create(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(flow)))
}

async fn read_flow(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FlowId>,
) -> Result<Json<Flow>, HttpError> {
    require_automation_grant(
        &state,
        &context,
        Action::View,
        "AutomationFlow",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let flow = state
        .flows
        .get(&mut transaction, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(flow))
}

async fn update_flow(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FlowId>,
    Json(input): Json<UpdateFlow>,
) -> Result<Json<Flow>, HttpError> {
    require_automation_grant(
        &state,
        &context,
        Action::Write,
        "AutomationFlow",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let flow = state
        .flows
        .update(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(flow))
}

async fn delete_flow(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FlowId>,
) -> Result<StatusCode, HttpError> {
    require_automation_grant(
        &state,
        &context,
        Action::Write,
        "AutomationFlow",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .flows
        .delete(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn simulate_flow(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FlowId>,
    Json(input): Json<SimulateFlow>,
) -> Result<Json<Simulation>, HttpError> {
    require_automation_grant(
        &state,
        &context,
        Action::View,
        "AutomationFlow",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let steps = state
        .flows
        .simulate(&mut transaction, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(Simulation { steps }))
}

#[derive(Clone, Debug, Serialize)]
struct Simulation {
    steps: Vec<SimulationStep>,
}

async fn list_flow_runs(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FlowId>,
    Query(filter): Query<RunListFilter>,
) -> Result<Json<Page<FlowRun>>, HttpError> {
    require_automation_grant(
        &state,
        &context,
        Action::View,
        "AutomationFlow",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .flows
        .list_runs(&mut transaction, id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn read_flow_run(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FlowRunId>,
) -> Result<Json<FlowRun>, HttpError> {
    require_automation_grant(
        &state,
        &context,
        Action::View,
        "AutomationRun",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let run = state
        .flows
        .get_run(&mut transaction, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(run))
}
