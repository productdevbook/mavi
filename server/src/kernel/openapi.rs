//! The description of the API, built from what the router was given.
//!
//! Every endpoint declares what it takes and gives, so the description is not
//! written by hand and cannot drift from what is served. The panel's types are
//! generated from it, and a test refuses a description that lies: two shapes
//! under one name, a listing that cannot be paged, a page declared as the
//! thing it holds.
use utoipa::openapi::path::{HttpMethod, Operation, OperationBuilder};
use utoipa::openapi::request_body::RequestBodyBuilder;
use utoipa::openapi::{
    Components, Content, InfoBuilder, OpenApi, OpenApiBuilder, PathItem, PathsBuilder, Required,
    ResponseBuilder, ResponsesBuilder,
};

use super::http::{Audience, Endpoint};

/// Names that two different types both answer to.
///
/// A description holds one schema per name, so two types called the same thing
/// leave only the last one — and a client generated from it is quietly wrong
/// about the other. Three types here were called `Credentials`.
#[must_use]
pub fn clashes(endpoints: &[Endpoint]) -> Vec<String> {
    let mut seen: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut both = Vec::new();

    for endpoint in endpoints {
        for (name, schema) in &endpoint.shape.named {
            let written = serde_json::to_string(schema).unwrap_or_default();

            match seen.get(name) {
                Some(before) if *before != written && !both.contains(name) => {
                    both.push(name.clone());
                }
                Some(_) => {}
                None => {
                    seen.insert(name.clone(), written);
                }
            }
        }
    }

    both
}

/// Built from the list of endpoints rather than beside it. There is one place
/// that says what this build serves, and this reads it.
#[must_use]
pub fn describe(endpoints: &[Endpoint]) -> OpenApi {
    let mut paths = PathsBuilder::new();
    let mut components = Components::new();

    for endpoint in endpoints {
        for (name, schema) in &endpoint.shape.named {
            components.schemas.insert(name.clone(), schema.clone());
        }

        paths = paths.path(endpoint.path(), item(endpoint));
    }

    OpenApiBuilder::new()
        .info(
            InfoBuilder::new()
                .title("Mavi CMS")
                .version(env!("CARGO_PKG_VERSION"))
                .build(),
        )
        .paths(paths.build())
        .components(Some(components))
        .build()
}

fn item(endpoint: &Endpoint) -> PathItem {
    PathItem::new(kind(endpoint.method()), operation(endpoint))
}

fn kind(method: &str) -> HttpMethod {
    match method {
        "post" => HttpMethod::Post,
        "put" => HttpMethod::Put,
        "patch" => HttpMethod::Patch,
        "delete" => HttpMethod::Delete,
        _ => HttpMethod::Get,
    }
}

fn operation(endpoint: &Endpoint) -> Operation {
    let guard = endpoint.guard();

    // Written into the description because it is the question a reader of this
    // file actually has: who may call it, and what happens if they call it too
    // often.
    let who = match guard.audience {
        Audience::Public => "Anybody.".to_owned(),
        Audience::Operator => "Whoever runs the machine, on its own screens.".to_owned(),
        Audience::Student => "A signed-in student.".to_owned(),
        Audience::User => match guard.needs {
            Some(needs) => format!("A panel account holding `{}`.", needs.grant()),
            None => "Any signed-in panel account.".to_owned(),
        },
    };

    let mut operation = OperationBuilder::new().description(Some(who));

    if let Some(takes) = endpoint.shape.takes.clone() {
        operation = operation.request_body(Some(
            RequestBodyBuilder::new()
                .content("application/json", Content::new(Some(takes)))
                .required(Some(Required::True))
                .build(),
        ));
    }

    let gives = endpoint.shape.gives.clone().map_or_else(
        || ResponseBuilder::new().description("Done.").build(),
        |schema| {
            ResponseBuilder::new()
                .description("What was asked for.")
                .content("application/json", Content::new(Some(schema)))
                .build()
        },
    );

    operation
        .responses(ResponsesBuilder::new().response("200", gives).build())
        .build()
}
