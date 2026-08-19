//! The committed artifacts are the contract consumed outside the Rust
//! workspace. This test makes a stale OpenAPI/client/MCP file a CI failure.

use mavi_http::api;

const OPENAPI: &str = include_str!("../contracts/mavi.openapi.json");
const TYPESCRIPT: &str = include_str!("../contracts/mavi.ts");
const RUST_CLIENT: &str = include_str!("../contracts/mavi_client.rs");
const MCP: &str = include_str!("../contracts/mcp-tools.json");

#[allow(dead_code)]
mod generated_rust {
    include!("../contracts/mavi_client.rs");
}

#[test]
fn committed_openapi_snapshot_matches_the_canonical_catalog() {
    let expected = format!(
        "{}\n",
        serde_json::to_string_pretty(&api().openapi("Mavi", "0.1.0").expect("OpenAPI"))
            .expect("OpenAPI JSON")
    );
    assert_eq!(OPENAPI, expected);
}

#[test]
fn committed_typescript_client_matches_the_canonical_catalog() {
    assert_eq!(TYPESCRIPT, api().typescript().expect("TypeScript client"));
}

#[test]
fn committed_rust_client_matches_the_canonical_catalog() {
    assert_eq!(RUST_CLIENT, api().rust_client().expect("Rust client"));
}

#[test]
fn generated_rust_client_is_valid_rust_and_contains_cursor_operations() {
    assert!(
        generated_rust::OPERATIONS
            .iter()
            .any(|operation| operation.name == "people.list"
                && operation.request == Some("PeopleListFilter"))
    );
}

#[test]
fn committed_mcp_tools_match_the_canonical_catalog() {
    let expected = format!(
        "{}\n",
        serde_json::to_string_pretty(&api().mcp_tools().expect("MCP tools")).expect("MCP JSON")
    );
    assert_eq!(MCP, expected);
}
