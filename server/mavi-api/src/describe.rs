//! The description, generated from what the endpoints said.
//!
//! Nothing is written here that an endpoint did not declare, and nothing an
//! endpoint declared is left out. That is the only way a description stays
//! true: the alternative is a second document somebody updates, and somebody
//! stops.

use mavi_core::error::Code;
use serde_json::{Map, Value, json};

use crate::{Api, Endpoint, In, Parameter, Who};

/// The whole description.
#[must_use]
pub fn openapi(api: &Api, version: &str) -> Value {
    let mut paths: Map<String, Value> = Map::new();

    for endpoint in &api.endpoints {
        let entry = paths
            .entry(endpoint.path.to_owned())
            .or_insert_with(|| json!({}));

        if let Some(object) = entry.as_object_mut() {
            object.insert(endpoint.method.lower().to_owned(), operation(endpoint));
        }
    }

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Mavi",
            "version": version,
        },
        // Said once, at the top, because a client that cannot authenticate
        // cannot do anything else either. The API it replaces described none.
        "components": {
            "securitySchemes": {
                "token": {
                    "type": "http",
                    "scheme": "bearer",
                    "description":
                        "A key made in the panel, or the token signing in answers with. What \
                         an assistant or a script uses.",
                },
                "session": {
                    "type": "apiKey",
                    "in": "cookie",
                    "name": "mavi_session",
                    "description":
                        "What the panel itself uses. A change made with one is asked \
                         where it came from.",
                },
            },
            "schemas": schemas(api),
        },
        "paths": paths,
    })
}

/// Every body this API has, by name.
///
/// The refusal is here rather than declared by a domain because no domain owns
/// it: it is what the guard and the router answer, and every operation refers
/// to it.
fn schemas(api: &Api) -> Value {
    let mut schemas = Map::new();

    schemas.insert("Refusal".to_owned(), refusal());

    for shape in &api.shapes {
        schemas.insert(shape.named.to_owned(), shape.described());
    }

    Value::Object(schemas)
}

fn operation(endpoint: &Endpoint) -> Value {
    let mut responses = Map::new();

    responses.insert(
        endpoint.answers.status().to_string(),
        match endpoint.answers.body() {
            Some(name) => json!({
                "description": "What it answers with.",
                "content": { "application/json": { "schema": { "$ref": format!("#/components/schemas/{name}") } } },
            }),
            None => json!({ "description": "Done. Nothing to say about it." }),
        },
    );

    // Everything it can refuse with: what it declared, and what the guard
    // above it answers for everybody. A caller reads one list.
    let mut refusals: Vec<Code> = Api::floor(endpoint);
    refusals.extend_from_slice(endpoint.refuses);
    refusals.sort_by_key(|code| code.status());
    refusals.dedup_by_key(|code| code.status());

    for code in refusals {
        responses.insert(
            code.status().to_string(),
            json!({
                "description": "Refused. `key` says which refusal, and is stable.",
                "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Refusal" } } },
            }),
        );
    }

    let tag = endpoint.named.split('.').next().unwrap_or("general");

    let mut operation = json!({
        "operationId": endpoint.named,
        "summary": endpoint.about,
        "tags": [tag],
        "responses": responses,
    });

    let object = operation.as_object_mut().expect("an object");

    if !endpoint.parameters.is_empty() {
        object.insert(
            "parameters".to_owned(),
            Value::Array(endpoint.parameters.iter().map(parameter).collect()),
        );
    }

    if let Some(takes) = endpoint.takes {
        // An upload is bytes, and describing it as JSON with a schema is how a
        // generated client comes to send a picture as a string. There is no
        // shape for it and there never will be — what a file is gets decided
        // by reading it rather than by anybody declaring it.
        let content = if takes == crate::THE_BYTES {
            json!({
                "application/octet-stream": {
                    "schema": { "type": "string", "format": "binary" },
                },
            })
        } else {
            json!({
                "application/json": {
                    "schema": { "$ref": format!("#/components/schemas/{takes}") },
                },
            })
        };

        object.insert(
            "requestBody".to_owned(),
            json!({ "required": true, "content": content }),
        );
    }

    object.insert(
        "security".to_owned(),
        match endpoint.who {
            // An empty requirement is how OpenAPI says "and this one needs
            // nothing", which is different from saying nothing at all.
            Who::Anybody => json!([{}]),
            _ => json!([{ "token": [] }, { "session": [] }]),
        },
    );

    operation
}

fn parameter(parameter: &Parameter) -> Value {
    let mut schema = json!({ "type": parameter.is.json() });

    if let Some(format) = parameter.is.format() {
        schema
            .as_object_mut()
            .expect("an object")
            .insert("format".to_owned(), json!(format));
    }

    json!({
        "name": parameter.name,
        "in": match parameter.carried { In::Path => "path", In::Query => "query" },
        "required": parameter.required,
        "description": parameter.about,
        "schema": schema,
    })
}

/// One shape for every refusal, so a client parses one thing.
fn refusal() -> Value {
    json!({
        "type": "object",
        "required": ["key", "named", "said"],
        "properties": {
            "key": {
                "type": "string",
                "description":
                    "Which refusal, exactly. Stable, and what a panel words in somebody's \
                     own language.",
            },
            "named": {
                "type": "object",
                "additionalProperties": { "type": "string" },
                "description": "What the sentence needs: a name, a count, a limit.",
            },
            "said": {
                "type": "string",
                "description":
                    "The English, for anything with no wording of its own. Never the only \
                     thing there, because whoever reads it may not read English.",
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Answers, Endpoint, Is, Method, Parameter};

    fn an_api() -> Api {
        Api::of(vec![
            Endpoint {
                method: Method::Get,
                path: "/api/posts/{id}",
                named: "posts.read",
                about: "One post.",
                who: Who::AnAccount,
                parameters: vec![Parameter::path("id", Is::Id, "Which post.")],
                takes: None,
                answers: Answers::With("Post"),
                refuses: &[Code::NotFound],
                changes: false,
            },
            Endpoint {
                method: Method::Post,
                path: "/api/posts",
                named: "posts.write",
                about: "Writes one.",
                who: Who::AnAccount,
                parameters: Vec::new(),
                takes: Some("NewPost"),
                answers: Answers::Made("Post"),
                refuses: &[Code::Conflict],
                changes: true,
            },
        ])
    }

    #[test]
    fn every_operation_says_how_to_authenticate() {
        let described = openapi(&an_api(), "0.0.0");
        let paths = described["paths"].as_object().expect("paths");

        for (path, methods) in paths {
            for (method, operation) in methods.as_object().expect("methods") {
                assert!(
                    operation.get("security").is_some(),
                    "{method} {path} does not say how to authenticate"
                );
            }
        }

        assert!(
            described["components"]["securitySchemes"]["token"].is_object(),
            "nothing described a token"
        );
    }

    #[test]
    fn every_operation_describes_the_status_it_answers_and_the_ones_it_refuses_with() {
        let described = openapi(&an_api(), "0.0.0");

        let reading = &described["paths"]["/api/posts/{id}"]["get"]["responses"];
        assert!(reading["200"].is_object(), "no success described");
        for refused in ["401", "403", "404", "422", "429", "500"] {
            assert!(reading[refused].is_object(), "{refused} not described");
        }

        // The one that used to be wrong sixty-seven times: a create answers
        // 201, and says so.
        let writing = &described["paths"]["/api/posts"]["post"]["responses"];
        assert!(writing["201"].is_object(), "a create described 200");
        assert!(writing["200"].is_null(), "a create described 200 as well");
    }

    #[test]
    fn a_templated_path_describes_the_thing_in_the_template() {
        let described = openapi(&an_api(), "0.0.0");
        let parameters = described["paths"]["/api/posts/{id}"]["get"]["parameters"]
            .as_array()
            .expect("parameters");

        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0]["name"], "id");
        assert_eq!(parameters[0]["in"], "path");
        assert_eq!(parameters[0]["required"], true);
        assert_eq!(parameters[0]["schema"]["format"], "uuid");
    }

    #[test]
    fn an_upload_is_described_as_bytes_rather_than_as_json() {
        let described = openapi(
            &Api::of(vec![Endpoint {
                method: Method::Post,
                path: "/api/files",
                named: "files.upload",
                about: "Takes one.",
                who: Who::AnAccount,
                parameters: Vec::new(),
                takes: Some(crate::THE_BYTES),
                answers: Answers::Made("File"),
                refuses: &[],
                changes: true,
            }]),
            "0.0.0",
        );

        let body = &described["paths"]["/api/files"]["post"]["requestBody"]["content"];

        // Described as JSON with a schema, this is how a generated client
        // comes to send somebody's picture as a string.
        assert!(body["application/json"].is_null());
        assert_eq!(
            body["application/octet-stream"]["schema"]["format"],
            "binary"
        );
    }

    #[test]
    fn there_is_one_shape_of_refusal() {
        let described = openapi(&an_api(), "0.0.0");
        let refusals = described["components"]["schemas"]["Refusal"]["properties"]
            .as_object()
            .expect("a refusal");

        // What is here is what a client branches on. Whether it is also what
        // comes back is asked where both halves are in one place — a
        // description nobody compares to the thing it describes is how this
        // said `error.code` for as long as it did while nothing ever sent one.
        for named in ["key", "named", "said"] {
            assert!(refusals.contains_key(named), "a refusal has no {named}");
        }
    }
}
