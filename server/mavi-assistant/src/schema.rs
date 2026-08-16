//! What a tool takes, out of what the endpoint already says it takes.
//!
//! An endpoint declares its holes and its query, each with a type and a
//! sentence. That is exactly what an assistant needs to be told, so it is
//! handed over rather than written again.
//!
//! The body is the one part that is not carried across whole. What an endpoint
//! takes has a **name** in the description — `WritingChanges` — and the shapes
//! behind those names are not written down yet, so there is nothing here to
//! turn into a schema. Until they are, `body` is an object with the name of
//! the shape said in its description, which is honest: an assistant is told
//! what to send and not told its fields.

use mavi_api::{Endpoint, In};
use serde_json::{Map, Value, json};

/// The name of the one argument that is not a parameter.
pub const BODY: &str = "body";

/// What a tool takes.
#[must_use]
pub fn takes(endpoint: &Endpoint) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();

    for parameter in &endpoint.parameters {
        // Where it is carried is said out loud. An assistant sends one flat
        // object, so nothing else would tell it that one of these becomes part
        // of the address and another narrows what comes back.
        let where_it_goes = match parameter.carried {
            In::Path => "part of the address",
            In::Query => "narrows what comes back",
        };

        let mut said = json!({
            "type": parameter.is.json(),
            "description": format!("{} ({where_it_goes})", parameter.about),
        });

        if let Some(format) = parameter.is.format() {
            said["format"] = json!(format);
        }

        if parameter.required {
            required.push(parameter.name);
        }

        properties.insert(parameter.name.to_owned(), said);
    }

    if let Some(takes) = endpoint.takes {
        properties.insert(
            BODY.to_owned(),
            json!({
                "type": "object",
                "description": format!(
                    "What this takes, shaped like {takes}. Its fields are not \
                     described here yet."
                ),
            }),
        );

        required.push(BODY);
    }

    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        // An argument nobody described is one nothing reads, and answering
        // "that was not a field" is more use than answering nothing about it.
        "additionalProperties": false,
    })
}

/// The pieces an endpoint is called with, out of what an assistant sent.
///
/// The holes in the path by name, the rest as a query, and the body as it was
/// given. Anything the endpoint did not declare is dropped rather than passed
/// on: what an assistant invents is not a parameter, and carrying it through
/// would make the tool's declared shape a suggestion.
#[must_use]
pub fn pieces(endpoint: &Endpoint, arguments: &Value) -> (Vec<(String, String)>, String, Value) {
    let mut path = Vec::new();
    let mut query = Vec::new();

    for parameter in &endpoint.parameters {
        let Some(given) = arguments.get(parameter.name) else {
            continue;
        };

        // A number arrives as a number and a hole in an address is text, so
        // what is sent is what it would have looked like written down.
        let said = match given {
            Value::String(said) => said.clone(),
            Value::Null => continue,
            other => other.to_string(),
        };

        match parameter.carried {
            In::Path => path.push((parameter.name.to_owned(), said)),
            In::Query => query.push(format!("{}={}", escaped(parameter.name), escaped(&said))),
        }
    }

    let body = arguments.get(BODY).cloned().unwrap_or(Value::Null);

    (path, query.join("&"), body)
}

/// A value on its way into a query string.
///
/// Written here rather than pulled in, because what has to survive is small
/// and known: the separators a query is made of, and the space. Everything
/// else a caller sends is its own business.
fn escaped(said: &str) -> String {
    let mut out = String::with_capacity(said.len());

    for byte in said.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_api::{Answers, Is, Method, Parameter, Who};

    fn an_endpoint(parameters: Vec<Parameter>, takes: Option<&'static str>) -> Endpoint {
        Endpoint {
            method: Method::Post,
            path: "/api/writings/{id}",
            named: "writings.change",
            about: "Changes one.",
            who: Who::AnAccount,
            parameters,
            takes,
            answers: Answers::With("Writing"),
            refuses: &[],
            changes: true,
        }
    }

    #[test]
    fn what_a_tool_takes_is_what_the_endpoint_said_it_takes() {
        let endpoint = an_endpoint(
            vec![
                Parameter::path("id", Is::Id, "Which one."),
                Parameter::query("after", Is::Text, "Where the last page stopped."),
            ],
            Some("WritingChanges"),
        );

        let takes = takes(&endpoint);

        assert_eq!(takes["properties"]["id"]["type"], "string");
        assert_eq!(takes["properties"]["id"]["format"], "uuid");
        assert_eq!(takes["properties"]["after"]["type"], "string");
        assert_eq!(takes["properties"][BODY]["type"], "object");

        // A hole in an address is always required; a narrowing is not.
        let required: Vec<&str> = takes["required"]
            .as_array()
            .expect("a list")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();

        assert!(required.contains(&"id"));
        assert!(required.contains(&BODY));
        assert!(!required.contains(&"after"));
    }

    #[test]
    fn an_endpoint_that_takes_nothing_asks_for_no_body() {
        let takes = takes(&an_endpoint(Vec::new(), None));

        assert!(takes["properties"].get(BODY).is_none());
    }

    #[test]
    fn arguments_become_the_pieces_the_endpoint_declared() {
        let endpoint = an_endpoint(
            vec![
                Parameter::path("id", Is::Id, "Which one."),
                Parameter::query("how_many", Is::Number, "How many."),
            ],
            Some("WritingChanges"),
        );

        let (path, query, body) = pieces(
            &endpoint,
            &json!({
                "id": "0193f00d-0000-7000-8000-000000000001",
                "how_many": 20,
                "body": { "title": "Something Else" },
                "invented": "whatever",
            }),
        );

        assert_eq!(
            path,
            vec![(
                "id".to_owned(),
                "0193f00d-0000-7000-8000-000000000001".to_owned()
            )]
        );
        // A number written down, because a hole in an address is text.
        assert_eq!(query, "how_many=20");
        assert_eq!(body["title"], "Something Else");
    }

    #[test]
    fn what_an_assistant_invented_is_not_a_parameter() {
        // The declared shape is the shape. Carrying something through because
        // it was sent would make "additionalProperties: false" a suggestion.
        let endpoint = an_endpoint(vec![Parameter::query("after", Is::Text, "Where.")], None);

        let (path, query, body) = pieces(&endpoint, &json!({ "invented": "whatever" }));

        assert!(path.is_empty());
        assert_eq!(query, "");
        assert_eq!(body, Value::Null);
    }

    #[test]
    fn what_would_come_apart_in_a_query_is_written_so_it_does_not() {
        let endpoint = an_endpoint(vec![Parameter::query("look", Is::Text, "For what.")], None);

        let (_, query, _) = pieces(&endpoint, &json!({ "look": "a & b = c" }));

        assert_eq!(query, "look=a%20%26%20b%20%3D%20c");
    }
}
