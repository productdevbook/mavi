mod support;

use axum::{http::Method, http::StatusCode};
use mavi_http::api;
use serde_json::json;
use support::{bootstrap, response_json, send};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn automation_http_contract_and_authorization_are_explicit() {
    let app = support::build_app().await;
    let token = bootstrap(&app, "Automation site").await;

    let template_id = Uuid::now_v7();
    let created = send(
        &app,
        Method::POST,
        "/api/v1/automation/flows",
        Some(&token),
        Some(json!({
            "name": "Welcome flow",
            "trigger": "form_submitted",
            "steps": [
                {"kind": "send_mail", "config": {"template_id": template_id}},
                {"kind": "wait", "config": {"seconds": 60}}
            ]
        })),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created_body = response_json(created).await;
    let flow_id = created_body["id"].as_str().expect("flow id");

    let enabled = send(
        &app,
        Method::PATCH,
        &format!("/api/v1/automation/flows/{flow_id}"),
        Some(&token),
        Some(json!({"enabled": true})),
    )
    .await;
    assert_eq!(enabled.status(), StatusCode::OK);

    let listed = send(
        &app,
        Method::GET,
        "/api/v1/automation/flows?limit=1",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed_body = response_json(listed).await;
    assert_eq!(listed_body["items"].as_array().expect("items").len(), 1);
    assert!(listed_body.get("offset").is_none());
    assert!(listed_body.get("page").is_none());

    let simulation = send(
        &app,
        Method::POST,
        &format!("/api/v1/automation/flows/{flow_id}/simulate"),
        Some(&token),
        Some(json!({"event": {"submission_id": "one"}})),
    )
    .await;
    assert_eq!(simulation.status(), StatusCode::OK);
    assert_eq!(
        response_json(simulation).await["steps"]
            .as_array()
            .expect("simulation steps")
            .len(),
        2
    );

    let trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/automation/flows/{flow_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(trashed.status(), StatusCode::NO_CONTENT);
    let flow_trash = send(
        &app,
        Method::GET,
        "/api/v1/trash?kind=flow",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(flow_trash.status(), StatusCode::OK);
    assert_eq!(response_json(flow_trash).await["items"][0]["id"], flow_id);
    let restored = send(
        &app,
        Method::POST,
        &format!("/api/v1/trash/flow/{flow_id}/restore"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(restored.status(), StatusCode::NO_CONTENT);
    let restored_flow = send(
        &app,
        Method::GET,
        &format!("/api/v1/automation/flows/{flow_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(restored_flow.status(), StatusCode::OK);
    assert_eq!(response_json(restored_flow).await["enabled"], true);

    let triggers = send(
        &app,
        Method::GET,
        "/api/v1/automation/triggers",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(triggers.status(), StatusCode::OK);
    assert_eq!(
        response_json(triggers)
            .await
            .as_array()
            .expect("triggers")
            .len(),
        6
    );

    let anonymous = send(&app, Method::GET, "/api/v1/automation/flows", None, None).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let catalog = api();
    assert!(
        catalog
            .endpoints
            .iter()
            .any(|endpoint| endpoint.operation_id == "automation.flows.list")
    );
    assert!(
        catalog
            .endpoints
            .iter()
            .any(|endpoint| endpoint.operation_id == "jobs.retry")
    );
}
