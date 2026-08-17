//! Emits the canonical API artifacts consumed by the panel, documentation and
//! MCP adapters.

use std::{env, process::ExitCode};

use mavi_http::api;

fn main() -> ExitCode {
    let format = env::args().nth(1).unwrap_or_else(|| "openapi".to_owned());
    let catalog = api();
    let result = match format.as_str() {
        "openapi" => catalog.openapi("Mavi", "0.1.0"),
        "mcp" => catalog.mcp_tools(),
        "typescript" => catalog.typescript().map(serde_json::Value::String),
        "rust" => catalog.rust_client().map(serde_json::Value::String),
        "json" => catalog.as_json().map_err(|error| vec![error.to_string()]),
        _ => Err(vec![format!(
            "unknown contract format: {format}; expected openapi, typescript, rust, mcp or json"
        )]),
    };

    match result {
        Ok(value) if matches!(format.as_str(), "typescript" | "rust") => {
            print!("{}", value.as_str().unwrap_or_default());
            ExitCode::SUCCESS
        }
        Ok(value) => match serde_json::to_string_pretty(&value) {
            Ok(value) => {
                println!("{value}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("could not serialize contract: {error}");
                ExitCode::FAILURE
            }
        },
        Err(errors) => {
            for error in errors {
                eprintln!("contract error: {error}");
            }
            ExitCode::FAILURE
        }
    }
}
