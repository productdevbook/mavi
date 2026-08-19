mod support;

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
    response::Response,
};
use serde_json::{Value, json};
use support::{bootstrap, response_json};
use tower::ServiceExt;

const PROTOCOL: &str = "2026-07-28";

async fn send_mcp(
    app: &axum::Router,
    token: Option<&str>,
    method: &str,
    id: i64,
    params: Value,
    name: Option<&str>,
) -> Response {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("content-type", "application/json")
        .header("MCP-Protocol-Version", PROTOCOL)
        .header("Mcp-Method", method);
    if let Some(name) = name {
        request = request.header("Mcp-Name", name);
    }
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    app.clone()
        .oneshot(
            request
                .body(Body::from(
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "method": method,
                        "params": params,
                    })
                    .to_string(),
                ))
                .expect("MCP request"),
        )
        .await
        .expect("MCP response")
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn mcp_is_stateless_cursor_based_and_reuses_http_authorization() {
    let app = support::build_app().await;
    let token = bootstrap(&app, "MCP site").await;

    let discover = send_mcp(&app, Some(&token), "server/discover", 1, json!({}), None).await;
    let discover_status = discover.status();
    let discovered = response_json(discover).await;
    assert_eq!(discover_status, StatusCode::OK, "{discovered}");
    assert_eq!(discovered["result"]["supportedVersions"][0], PROTOCOL);
    assert_eq!(
        discovered["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "mavi"
    );

    let tools = send_mcp(&app, Some(&token), "tools/list", 2, json!({}), None).await;
    assert_eq!(tools.status(), StatusCode::OK);
    let tools = response_json(tools).await;
    assert!(
        tools["result"]["tools"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(tools["result"].get("offset").is_none());
    assert!(tools["result"].get("page").is_none());
    if let Some(cursor) = tools["result"]["nextCursor"].as_str() {
        let next = send_mcp(
            &app,
            Some(&token),
            "tools/list",
            6,
            json!({"cursor": cursor}),
            None,
        )
        .await;
        assert_eq!(next.status(), StatusCode::OK);
        let next = response_json(next).await;
        assert!(
            next["result"]["tools"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert_ne!(
            tools["result"]["tools"][0]["name"],
            next["result"]["tools"][0]["name"]
        );
    }

    let created = send_mcp(
        &app,
        Some(&token),
        "tools/call",
        3,
        json!({
            "name": "content.create",
            "arguments": {
                "body": {
                    "kind": "post",
                    "language": "en",
                    "slug": "mcp-post",
                    "title": "MCP post",
                    "excerpt": null,
                    "body": "Created through MCP",
                    "fields": {},
                    "publication": "draft"
                }
            }
        }),
        Some("content.create"),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let created = response_json(created).await;
    assert_eq!(created["result"]["isError"], false);
    assert_eq!(created["result"]["structuredContent"]["slug"], "mcp-post");

    let anonymous = send_mcp(&app, None, "tools/list", 4, json!({}), None).await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let mismatch = send_mcp(
        &app,
        Some(&token),
        "tools/call",
        5,
        json!({"name": "content.create", "arguments": {"body": {}}}),
        Some("people.list"),
    )
    .await;
    assert_eq!(mismatch.status(), StatusCode::BAD_REQUEST);
    let mismatch = to_bytes(mismatch.into_body(), 1024 * 1024)
        .await
        .expect("MCP error body");
    let mismatch: Value = serde_json::from_slice(&mismatch).expect("MCP error JSON");
    assert_eq!(mismatch["error"]["code"], -32020);
}
