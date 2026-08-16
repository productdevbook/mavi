//! The panel's types, written from the description of the API.
//!
//! Two hand-written copies of the same shape disagree the first time one of
//! them changes, and the one that is wrong is always the one nobody is looking
//! at. So there is one: this reads what the endpoints already say and writes
//! the other copy, and a test refuses to pass while the file on disk is not
//! what this produces.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::Value;

/// What the panel imports.
#[must_use]
pub fn of(api: &utoipa::openapi::OpenApi) -> String {
    let described =
        serde_json::to_value(api).unwrap_or_else(|_| Value::Object(serde_json::Map::new()));

    let mut out = String::from(
        "// Written from the description of the API. Do not edit by hand:\n\
         // `UPDATE_SNAPSHOTS=1 cargo test --test api` writes it.\n\n\
         export interface Page<T> {\n  items: T[];\n  next?: string | null;\n}\n\n",
    );

    let schemas = described
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    for (name, schema) in &schemas {
        // `Page` is written out above, once, as the generic it is: the
        // description carries one concrete page per listing and they are all
        // the same shape.
        if name == "Page" {
            continue;
        }

        out.push_str(&declared(name, schema));
        out.push('\n');
    }

    out.push_str(&calls(&described));
    out
}

fn declared(name: &str, schema: &Value) -> String {
    if let Some(one_of) = schema.get("oneOf").and_then(Value::as_array) {
        let arms: Vec<String> = one_of.iter().map(rendered).collect();
        return format!("export type {name} = {};\n", arms.join(" | "));
    }

    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        let arms: Vec<String> = values.iter().map(literal).collect();
        return format!("export type {name} = {};\n", arms.join(" | "));
    }

    if schema.get("properties").is_some() {
        return format!("export interface {name} {}\n", body(schema));
    }

    format!("export type {name} = {};\n", rendered(schema))
}

fn body(schema: &Value) -> String {
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut out = String::from("{\n");

    for (field, shape) in &properties {
        let optional = if required.contains(&field.as_str()) {
            ""
        } else {
            "?"
        };

        let _ = writeln!(out, "  {field}{optional}: {};", rendered(shape));
    }

    out.push('}');
    out
}

fn rendered(schema: &Value) -> String {
    // One pagination shape, said once: a listing is written as `Page<Post>`
    // rather than as its two fields spelled out at every listing.
    if let Some(of) = a_page_of(schema) {
        return format!("Page<{of}>");
    }

    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return reference.rsplit('/').next().unwrap_or("unknown").to_owned();
    }

    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        let arms: Vec<String> = values.iter().map(literal).collect();
        return arms.join(" | ");
    }

    if let Some(one_of) = schema.get("oneOf").and_then(Value::as_array) {
        let arms: Vec<String> = one_of.iter().map(rendered).collect();
        return arms.join(" | ");
    }

    match schema.get("type") {
        // A type given as a list is a nullable one, which is how utoipa says
        // "or null" — the union is the whole of it.
        Some(Value::Array(kinds)) => {
            let arms: Vec<String> = kinds
                .iter()
                .filter_map(Value::as_str)
                .map(|kind| named(kind, schema))
                .collect();

            arms.join(" | ")
        }
        Some(Value::String(kind)) => named(kind, schema),
        _ => "unknown".to_owned(),
    }
}

fn a_page_of(schema: &Value) -> Option<String> {
    let properties = schema.get("properties")?.as_object()?;

    if properties.len() != 2 || !properties.contains_key("next") {
        return None;
    }

    let items = properties.get("items")?;

    if items.get("type")?.as_str()? != "array" {
        return None;
    }

    Some(rendered(items.get("items")?))
}

fn named(kind: &str, schema: &Value) -> String {
    match kind {
        "string" => "string".to_owned(),
        "integer" | "number" => "number".to_owned(),
        "boolean" => "boolean".to_owned(),
        "null" => "null".to_owned(),
        "array" => {
            let of = schema.get("items").map_or_else(
                || "unknown".to_owned(),
                |items| {
                    let rendered = rendered(items);

                    if rendered.contains(' ') {
                        format!("({rendered})")
                    } else {
                        rendered
                    }
                },
            );

            format!("{of}[]")
        }
        "object" => {
            if schema.get("properties").is_some() {
                body(schema)
            } else if let Some(values) = schema.get("additionalProperties") {
                format!("Record<string, {}>", rendered(values))
            } else {
                "Record<string, unknown>".to_owned()
            }
        }
        other => other.to_owned(),
    }
}

/// Every call there is: what it takes and what it gives, keyed the way a client
/// would ask for it. A panel that asks for a path this does not name does not
/// compile, which is the point of writing it at all.
fn calls(described: &Value) -> String {
    let paths = described
        .pointer("/paths")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut lines: BTreeMap<String, (String, String)> = BTreeMap::new();

    for (path, item) in &paths {
        let Some(item) = item.as_object() else {
            continue;
        };

        for (method, operation) in item {
            if !matches!(method.as_str(), "get" | "post" | "put" | "patch" | "delete") {
                continue;
            }

            let takes = operation
                .pointer("/requestBody/content/application~1json/schema")
                .map_or_else(|| "never".to_owned(), rendered);

            let gives = operation
                .get("responses")
                .and_then(Value::as_object)
                .and_then(|answers| {
                    answers
                        .iter()
                        .filter(|(status, _)| status.starts_with('2'))
                        .map(|(_, answer)| answer)
                        .find_map(|answer| {
                            answer.pointer("/content/application~1json/schema").cloned()
                        })
                })
                .map_or_else(|| "void".to_owned(), |schema| rendered(&schema));

            lines.insert(format!("{} {path}", method.to_uppercase()), (takes, gives));
        }
    }

    let mut out = String::from("export interface Calls {\n");

    for (call, (takes, gives)) in lines {
        let _ = writeln!(out, "  \"{call}\": {{ takes: {takes}; gives: {gives} }};");
    }

    out.push_str("}\n");
    out
}

fn literal(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), |text| format!("\"{text}\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_shape_is_in_the_description_is_what_it_is_in_typescript() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["id", "count"],
            "properties": {
                "id": { "type": "string" },
                "count": { "type": "integer" },
                "note": { "type": ["string", "null"] },
                "tags": { "type": "array", "items": { "type": "string" } },
                "post": { "$ref": "#/components/schemas/Post" },
            },
        });

        let written = declared("Thing", &schema);

        assert!(written.contains("id: string;"));
        assert!(written.contains("count: number;"));
        assert!(written.contains("note?: string | null;"));
        assert!(written.contains("tags?: string[];"));
        assert!(written.contains("post?: Post;"));
    }

    #[test]
    fn a_listing_is_written_as_one_shape() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["items"],
            "properties": {
                "items": { "type": "array", "items": { "$ref": "#/components/schemas/Post" } },
                "next": { "type": ["string", "null"] },
            },
        });

        assert_eq!(rendered(&schema), "Page<Post>");
    }

    #[test]
    fn a_list_of_things_that_are_more_than_one_thing_is_bracketed() {
        let schema = serde_json::json!({
            "type": "array",
            "items": { "type": ["string", "null"] },
        });

        assert_eq!(rendered(&schema), "(string | null)[]");
    }

    #[test]
    fn a_set_of_named_values_is_a_union_rather_than_a_string() {
        let schema = serde_json::json!({ "type": "string", "enum": ["draft", "published"] });

        assert_eq!(rendered(&schema), "\"draft\" | \"published\"");
    }
}
