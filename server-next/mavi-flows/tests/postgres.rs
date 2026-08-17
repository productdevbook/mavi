use std::env;

use mavi_core::{PageRequest, SiteContext, SiteId};
use mavi_flows::{
    CreateFlow, FlowListFilter, FlowService, FlowStepInput, RecordStep, SimulateFlow, StartFlowJob,
    StepKind, StepOutcome, Trigger, UpdateFlow,
};
use mavi_jobs::JobsService;
use mavi_storage::Database;
use serde_json::json;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn flows_snapshot_events_and_steps_are_site_scoped() {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 4).await.expect("database");
    database.migrate().await.expect("migrations");
    let first_site = SiteId::new();
    let second_site = SiteId::new();
    database.ensure_site(first_site).await.expect("first site");
    database
        .ensure_site(second_site)
        .await
        .expect("second site");

    let context = SiteContext::public(first_site);
    let jobs = JobsService::new(mavi_flows::job_kinds());
    let service = FlowService;
    let flow_id = {
        let mut tx = database.begin(&context).await.expect("authoring scope");
        let flow = service
            .create(
                &mut tx,
                &context,
                &CreateFlow {
                    name: "Welcome automation".to_owned(),
                    trigger: Trigger::FormSubmitted,
                    steps: vec![FlowStepInput {
                        kind: StepKind::Wait,
                        config: json!({"seconds": 60}),
                    }],
                },
            )
            .await
            .expect("flow");
        assert!(!flow.enabled);
        let enabled = service
            .update(
                &mut tx,
                &context,
                flow.id,
                &UpdateFlow {
                    enabled: Some(true),
                    ..UpdateFlow::default()
                },
            )
            .await
            .expect("enable flow");
        assert!(enabled.enabled);
        let preview = service
            .simulate(
                &mut tx,
                flow.id,
                &SimulateFlow {
                    event: json!({"submission_id": "one"}),
                },
            )
            .await
            .expect("preview");
        assert_eq!(preview.len(), 1);
        tx.commit().await.expect("authoring commit");
        flow.id
    };

    let start_job = {
        let mut tx = database.begin(&context).await.expect("emit scope");
        let first = service
            .emit(
                &mut tx,
                &context,
                &jobs,
                Trigger::FormSubmitted,
                &json!({"submission_id": "one"}),
                Some("submission-one"),
            )
            .await
            .expect("emit");
        let duplicate = service
            .emit(
                &mut tx,
                &context,
                &jobs,
                Trigger::FormSubmitted,
                &json!({"submission_id": "one"}),
                Some("submission-one"),
            )
            .await
            .expect("duplicate emit");
        assert_eq!(first, duplicate);
        tx.commit().await.expect("emit commit");
        first[0]
    };

    let start = {
        let mut tx = database.begin(&context).await.expect("claim start scope");
        let claim = jobs
            .claim(
                &mut tx,
                "flow-worker",
                &[mavi_flows::FLOW_START_KIND.name],
                30,
            )
            .await
            .expect("claim start")
            .expect("start job");
        assert_eq!(claim.id, start_job);
        let input: StartFlowJob = serde_json::from_value(claim.payload).expect("start payload");
        tx.commit().await.expect("claim start commit");
        input
    };
    assert_eq!(start.flow_id, flow_id);

    let run = {
        let mut tx = database.begin(&context).await.expect("start scope");
        let run = service
            .start(&mut tx, &context, &jobs, &start)
            .await
            .expect("run");
        assert_eq!(run.definition.len(), 1);
        tx.commit().await.expect("start commit");
        run
    };

    let step_job = {
        let mut tx = database.begin(&context).await.expect("claim step scope");
        let claim = jobs
            .claim(
                &mut tx,
                "flow-worker",
                &[mavi_flows::FLOW_STEP_KIND.name],
                30,
            )
            .await
            .expect("claim step")
            .expect("step job");
        let input: mavi_flows::StepJob =
            serde_json::from_value(claim.payload).expect("step payload");
        tx.commit().await.expect("claim step commit");
        input
    };
    assert_eq!(step_job.run_id, run.id);

    {
        let mut tx = database.begin(&context).await.expect("record scope");
        let completed = service
            .record_step(
                &mut tx,
                &context,
                &jobs,
                &RecordStep {
                    run_id: run.id,
                    position: 0,
                    attempt: 1,
                    outcome: StepOutcome::Succeeded,
                    detail: json!({"sent": false}),
                    error: None,
                    next_at: None,
                },
            )
            .await
            .expect("record step");
        assert_eq!(completed.state, mavi_flows::RunState::Succeeded);
        tx.commit().await.expect("record commit");
    }

    {
        let mut tx = database.begin(&context).await.expect("list scope");
        let page = service
            .list(
                &mut tx,
                &FlowListFilter {
                    page: PageRequest {
                        after: None,
                        limit: Some(1),
                    },
                    trigger: None,
                    enabled: None,
                },
            )
            .await
            .expect("flow list");
        assert_eq!(page.items.len(), 1);
        let runs = service
            .list_runs(&mut tx, flow_id, &mavi_flows::RunListFilter::default())
            .await
            .expect("run list");
        assert_eq!(runs.items.len(), 1);
        tx.commit().await.expect("list commit");
    }

    let other_context = SiteContext::public(second_site);
    let mut tx = database
        .begin(&other_context)
        .await
        .expect("isolation scope");
    assert!(
        service
            .list(&mut tx, &mavi_flows::FlowListFilter::default())
            .await
            .expect("other list")
            .items
            .is_empty()
    );
    tx.commit().await.expect("isolation commit");
}
