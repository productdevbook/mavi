mod support;

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use serde_json::{Value, json};
use support::{bootstrap, response_json, send};
use tower::ServiceExt;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn plugin_activation_updates_routes_and_openapi_without_restart() {
    let app = support::build_app_with_default_plugins().await;
    let owner_token = bootstrap(&app, "HTTP plugin test").await;

    let records = send(
        &app,
        Method::GET,
        "/api/v1/plugins",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(records.status(), StatusCode::OK);
    let records = response_json(records)
        .await
        .as_array()
        .cloned()
        .expect("plugin records");
    assert!(records.iter().any(|record| record["id"] == "analytics"));
    assert!(
        records
            .iter()
            .any(|record| { record["id"] == "core" && record["enabled"] == Value::Bool(true) })
    );
    assert!(
        records.iter().any(|record| {
            record["id"] == "commerce" && record["enabled"] == Value::Bool(false)
        })
    );

    let disabled = send(
        &app,
        Method::GET,
        "/public/v1/shop/products?limit=1",
        None,
        None,
    )
    .await;
    assert_eq!(disabled.status(), StatusCode::NOT_FOUND);
    assert!(
        !mcp_tool_names(&app, &owner_token)
            .await
            .iter()
            .any(|name| name == "shop.products.list")
    );

    let openapi = send(&app, Method::GET, "/openapi.json", Some(&owner_token), None).await;
    assert_eq!(openapi.status(), StatusCode::OK);
    let openapi: Value = response_json(openapi).await;
    assert!(openapi["paths"].get("/api/v1/shop/products").is_none());

    let governance_enabled = send(
        &app,
        Method::POST,
        "/api/v1/plugins/governance/enable",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(governance_enabled.status(), StatusCode::OK);

    let governance_disabled = send(
        &app,
        Method::POST,
        "/api/v1/plugins/governance/disable",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(governance_disabled.status(), StatusCode::OK);

    let content_trash = send(
        &app,
        Method::DELETE,
        "/api/v1/content/00000000-0000-4000-8000-000000000001",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(content_trash.status(), StatusCode::NOT_FOUND);

    let governance_reenabled = send(
        &app,
        Method::POST,
        "/api/v1/plugins/governance/enable",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(governance_reenabled.status(), StatusCode::OK);

    let enabled = send(
        &app,
        Method::POST,
        "/api/v1/plugins/commerce/enable",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(enabled.status(), StatusCode::OK);
    assert_eq!(response_json(enabled).await["enabled"], true);

    let public = send(
        &app,
        Method::GET,
        "/public/v1/shop/products?limit=1",
        None,
        None,
    )
    .await;
    assert_eq!(public.status(), StatusCode::OK);
    assert!(
        mcp_tool_names(&app, &owner_token)
            .await
            .iter()
            .any(|name| name == "shop.products.list")
    );

    let openapi = send(&app, Method::GET, "/openapi.json", Some(&owner_token), None).await;
    assert_eq!(openapi.status(), StatusCode::OK);
    let openapi: Value = response_json(openapi).await;
    assert!(openapi["paths"].get("/api/v1/shop/products").is_some());

    let disabled = send(
        &app,
        Method::POST,
        "/api/v1/plugins/commerce/disable",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(disabled.status(), StatusCode::OK);
    assert_eq!(response_json(disabled).await["enabled"], false);

    let disabled_again = send(
        &app,
        Method::GET,
        "/public/v1/shop/products?limit=1",
        None,
        None,
    )
    .await;
    assert_eq!(disabled_again.status(), StatusCode::NOT_FOUND);
    assert!(
        !mcp_tool_names(&app, &owner_token)
            .await
            .iter()
            .any(|name| name == "shop.products.list")
    );

    let core_disable = send(
        &app,
        Method::POST,
        "/api/v1/plugins/core/disable",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(core_disable.status(), StatusCode::CONFLICT);
}

async fn mcp_tool_names(app: &Router, token: &str) -> Vec<String> {
    let mut cursor = None;
    let mut names = Vec::new();
    loop {
        let params = cursor
            .as_ref()
            .map_or_else(|| json!({}), |cursor| json!({"cursor": cursor}));
        let request = Request::builder()
            .method(Method::POST)
            .uri("/mcp")
            .header("content-type", "application/json")
            .header("MCP-Protocol-Version", "2026-07-28")
            .header("Mcp-Method", "tools/list")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/list",
                    "params": params
                })
                .to_string(),
            ))
            .expect("MCP request");
        let response = app.clone().oneshot(request).await.expect("MCP response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        names.extend(
            body["result"]["tools"]
                .as_array()
                .expect("MCP tools")
                .iter()
                .filter_map(|tool| tool["name"].as_str().map(str::to_owned)),
        );
        let Some(next) = body["result"]["nextCursor"].as_str() else {
            break;
        };
        cursor = Some(next.to_owned());
    }
    names
}
