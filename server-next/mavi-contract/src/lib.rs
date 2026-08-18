//! Canonical API declarations.
//!
//! A route is not complete unless it says what it takes, returns, refuses,
//! who may call it and whether it requires a site scope. `OpenAPI`, generated
//! clients and MCP tools are all built from this list rather than maintained
//! beside it.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

use mavi_core::{Action, Capability, ErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Post => "post",
            Self::Put => "put",
            Self::Patch => "patch",
            Self::Delete => "delete",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputLocation {
    Json,
    Query,
    Raw,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputLocation {
    #[default]
    Json,
    Raw,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_json_output(location: &OutputLocation) -> bool {
    *location == OutputLocation::Json
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestShape {
    pub shape: String,
    pub location: InputLocation,
}

impl RequestShape {
    #[must_use]
    pub fn json(shape: impl Into<String>) -> Self {
        Self {
            shape: shape.into(),
            location: InputLocation::Json,
        }
    }

    #[must_use]
    pub fn query(shape: impl Into<String>) -> Self {
        Self {
            shape: shape.into(),
            location: InputLocation::Query,
        }
    }

    #[must_use]
    pub fn raw(shape: impl Into<String>) -> Self {
        Self {
            shape: shape.into(),
            location: InputLocation::Raw,
        }
    }
}

/// A named JSON schema owned by the domain that declares the endpoint.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Shape {
    pub name: String,
    pub schema: Value,
}

impl Shape {
    #[must_use]
    pub fn new(name: impl Into<String>, schema: Value) -> Self {
        Self {
            name: name.into(),
            schema,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Authentication {
    Public,
    Account,
    AccountOrAssistant,
    Student,
    Assistant,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Permission {
    pub capability: Capability,
    pub action: Action,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mutation {
    #[default]
    None,
    Permissioned {
        idempotent: bool,
    },
    Public {
        idempotent: bool,
    },
    SelfOnly {
        idempotent: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    #[default]
    Site,
    ControlPlane,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Endpoint {
    pub method: Method,
    pub path: String,
    pub operation_id: String,
    pub summary: String,
    pub scope: Scope,
    pub authentication: Authentication,
    pub permission: Option<Permission>,
    pub request: Option<RequestShape>,
    pub query: Option<String>,
    pub response: Option<String>,
    #[serde(default, skip_serializing_if = "is_json_output")]
    pub response_location: OutputLocation,
    pub status: u16,
    pub errors: Vec<ErrorCode>,
    pub mutation: Mutation,
}

impl Endpoint {
    #[must_use]
    pub fn new(
        method: Method,
        path: impl Into<String>,
        operation_id: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            method,
            path: path.into(),
            operation_id: operation_id.into(),
            summary: summary.into(),
            scope: Scope::Site,
            authentication: Authentication::Account,
            permission: None,
            request: None,
            query: None,
            response: None,
            response_location: OutputLocation::Json,
            status: 200,
            errors: vec![ErrorCode::Internal],
            mutation: Mutation::None,
        }
    }

    #[must_use]
    pub const fn public(mut self) -> Self {
        self.authentication = Authentication::Public;
        self
    }

    #[must_use]
    pub const fn student(mut self) -> Self {
        self.authentication = Authentication::Student;
        self
    }

    #[must_use]
    pub const fn student_changes(mut self, idempotent: bool) -> Self {
        self.authentication = Authentication::Student;
        self.mutation = Mutation::SelfOnly { idempotent };
        self
    }

    #[must_use]
    pub const fn public_mutation(self) -> Self {
        self.public_changes(false)
    }

    #[must_use]
    pub const fn public_changes(mut self, idempotent: bool) -> Self {
        self.authentication = Authentication::Public;
        self.mutation = Mutation::Public { idempotent };
        self
    }

    #[must_use]
    pub const fn self_only(mut self) -> Self {
        self.mutation = Mutation::SelfOnly { idempotent: false };
        self
    }

    #[must_use]
    pub const fn account_or_assistant(mut self) -> Self {
        self.authentication = Authentication::AccountOrAssistant;
        self
    }

    #[must_use]
    pub const fn control_plane(mut self) -> Self {
        self.scope = Scope::ControlPlane;
        self
    }

    #[must_use]
    pub const fn requires(mut self, permission: Permission) -> Self {
        self.permission = Some(permission);
        self
    }

    #[must_use]
    pub fn takes(mut self, shape: impl Into<String>) -> Self {
        self.request = Some(RequestShape::json(shape));
        self
    }

    #[must_use]
    pub fn takes_query(mut self, shape: impl Into<String>) -> Self {
        self.request = Some(RequestShape::query(shape));
        self
    }

    #[must_use]
    pub fn takes_raw(mut self, shape: impl Into<String>) -> Self {
        self.request = Some(RequestShape::raw(shape));
        self
    }

    #[must_use]
    pub fn with_query(mut self, shape: impl Into<String>) -> Self {
        self.query = Some(shape.into());
        self
    }

    #[must_use]
    pub fn returns(mut self, status: u16, shape: impl Into<String>) -> Self {
        self.status = status;
        self.response = Some(shape.into());
        self
    }

    #[must_use]
    pub fn returns_raw(mut self, status: u16, shape: impl Into<String>) -> Self {
        self.status = status;
        self.response = Some(shape.into());
        self.response_location = OutputLocation::Raw;
        self
    }

    #[must_use]
    pub fn refuses(mut self, errors: impl IntoIterator<Item = ErrorCode>) -> Self {
        self.errors = errors.into_iter().collect();
        self
    }

    #[must_use]
    pub const fn changes(mut self, idempotent: bool) -> Self {
        self.mutation = Mutation::Permissioned { idempotent };
        self
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Api {
    pub endpoints: Vec<Endpoint>,
    pub shapes: Vec<Shape>,
}

impl Api {
    #[must_use]
    pub fn new(endpoints: impl IntoIterator<Item = Endpoint>) -> Self {
        Self {
            endpoints: endpoints.into_iter().collect(),
            shapes: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_shapes(mut self, shapes: impl IntoIterator<Item = Shape>) -> Self {
        self.shapes.extend(shapes);
        self
    }

    pub fn extend(&mut self, other: Self) {
        self.endpoints.extend(other.endpoints);
        for shape in other.shapes {
            let same_shape = self
                .shapes
                .iter()
                .find(|existing| existing.name == shape.name)
                .is_some_and(|existing| existing.schema == shape.schema);
            if !same_shape {
                self.shapes.push(shape);
            }
        }
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let mut operation_ids = std::collections::BTreeSet::new();
        let mut routes = std::collections::BTreeSet::new();

        for endpoint in &self.endpoints {
            if !operation_ids.insert(endpoint.operation_id.clone()) {
                errors.push(format!("duplicate operation id: {}", endpoint.operation_id));
            }

            let route = format!("{:?} {}", endpoint.method, endpoint.path);
            if !routes.insert(route.clone()) {
                errors.push(format!("duplicate route: {route}"));
            }

            if matches!(endpoint.mutation, Mutation::Permissioned { .. })
                && endpoint.permission.is_none()
            {
                errors.push(format!(
                    "mutation has no permission: {}",
                    endpoint.operation_id
                ));
            }

            if endpoint.scope == Scope::ControlPlane
                && endpoint.authentication == Authentication::Public
            {
                errors.push(format!(
                    "control-plane endpoint cannot be public: {}",
                    endpoint.operation_id
                ));
            }

            if endpoint.response.is_none() && endpoint.status < 200 {
                errors.push(format!(
                    "invalid response status: {}",
                    endpoint.operation_id
                ));
            }

            if endpoint.query.is_some()
                && endpoint
                    .request
                    .as_ref()
                    .is_some_and(|request| request.location == InputLocation::Query)
            {
                errors.push(format!(
                    "endpoint declares query input twice: {}",
                    endpoint.operation_id
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn as_json(&self) -> Result<Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    /// Returns the stable fingerprint of the canonical API declaration.
    ///
    /// The declaration is serialized in its authored order. Domain crates
    /// own that order, while the contract generator and runtime manifest use
    /// this same value, so an operator can reject a panel or client built for
    /// a different API without guessing from the product version alone.
    pub fn fingerprint(&self) -> Result<String, serde_json::Error> {
        let bytes = serde_json::to_vec(self)?;
        let digest = Sha256::digest(bytes);
        let mut hexadecimal = String::with_capacity(64);
        for byte in digest {
            write!(hexadecimal, "{byte:02x}").expect("writing to a String cannot fail");
        }
        Ok(format!("sha256:{hexadecimal}"))
    }

    #[allow(clippy::too_many_lines)]
    pub fn openapi(
        &self,
        title: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<Value, Vec<String>> {
        self.validate()?;

        let schemas = self.schemas()?;
        let mut paths = Map::new();
        for endpoint in &self.endpoints {
            let path = paths
                .entry(endpoint.path.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            let Some(path_item) = path.as_object_mut() else {
                return Err(vec![format!("invalid generated path: {}", endpoint.path)]);
            };

            let mut operation = Map::new();
            operation.insert(
                "operationId".to_owned(),
                Value::String(endpoint.operation_id.clone()),
            );
            operation.insert(
                "summary".to_owned(),
                Value::String(endpoint.summary.clone()),
            );

            operation.insert(
                "tags".to_owned(),
                json!([endpoint.operation_id.split('.').next().unwrap_or("general")]),
            );
            let mut responses = Map::new();
            responses.insert(endpoint.status.to_string(), success_response(endpoint));
            for error in &endpoint.errors {
                responses.entry(error_status(*error).to_string()).or_insert_with(|| {
                    json!({
                        "description": "Request refused",
                        "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ErrorEnvelope"}}}
                    })
                });
            }
            operation.insert("responses".to_owned(), Value::Object(responses));

            if let Some(request) = &endpoint.request {
                match request.location {
                    InputLocation::Json => {
                        operation.insert(
                            "requestBody".to_owned(),
                            json!({
                                "required": true,
                                "content": {"application/json": {"schema": schema_ref(&request.shape)}}
                            }),
                        );
                    }
                    InputLocation::Query => {
                        operation.insert(
                            "parameters".to_owned(),
                            Value::Array(query_parameters(self, &request.shape)?),
                        );
                    }
                    InputLocation::Raw => {
                        operation.insert(
                            "requestBody".to_owned(),
                            json!({
                                "required": true,
                                "content": {"application/octet-stream": {"schema": schema_ref(&request.shape)}}
                            }),
                        );
                    }
                }
            }

            if let Some(query) = &endpoint.query {
                let generated = query_parameters(self, query)?;
                operation
                    .entry("parameters".to_owned())
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Some(parameters) = operation
                    .get_mut("parameters")
                    .and_then(Value::as_array_mut)
                {
                    parameters.extend(generated);
                }
            }

            let path_parameters = path_parameters(&endpoint.path);
            if !path_parameters.is_empty() {
                let generated = path_parameters
                    .into_iter()
                    .map(|name| {
                        json!({
                            "name": name,
                            "in": "path",
                            "required": true,
                            "schema": if name == "id" { json!({"type": "string", "format": "uuid"}) } else { json!({"type": "string"}) },
                        })
                    })
                    .collect::<Vec<_>>();
                operation
                    .entry("parameters".to_owned())
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Some(parameters) = operation
                    .get_mut("parameters")
                    .and_then(Value::as_array_mut)
                {
                    let mut all = generated;
                    all.append(parameters);
                    *parameters = all;
                }
            }

            if endpoint.authentication == Authentication::Public {
                operation.insert("security".to_owned(), json!([{}]));
            } else {
                operation.insert("security".to_owned(), json!([{ "bearerAuth": [] }]));
            }
            operation.insert(
                "x-mavi".to_owned(),
                json!({
                    "scope": endpoint.scope,
                    "authentication": endpoint.authentication,
                    "mutation": endpoint.mutation,
                    "permission": endpoint.permission,
                    "errors": endpoint.errors,
                }),
            );
            path_item.insert(
                endpoint.method.as_str().to_owned(),
                Value::Object(operation),
            );
        }

        let mut component_schemas = Map::new();
        for (name, schema) in schemas {
            component_schemas.insert(name, schema);
        }

        Ok(json!({
            "openapi": "3.1.0",
            "info": {"title": title.into(), "version": version.into()},
            "paths": paths,
            "components": {
                "securitySchemes": {"bearerAuth": {"type": "http", "scheme": "bearer"}},
                "schemas": component_schemas,
            }
        }))
    }

    fn schemas(&self) -> Result<BTreeMap<String, Value>, Vec<String>> {
        let mut schemas = BTreeMap::from([
            (
                "Empty".to_owned(),
                json!({"type": "object", "additionalProperties": false}),
            ),
            (
                "ErrorBody".to_owned(),
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["code", "message"],
                    "properties": {
                        "code": {"type": "string"},
                        "message": {"type": "string"},
                        "field": {"type": ["string", "null"]},
                    }
                }),
            ),
            (
                "ErrorEnvelope".to_owned(),
                json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["error"],
                    "properties": {"error": {"$ref": "#/components/schemas/ErrorBody"}},
                }),
            ),
        ]);
        let mut errors = Vec::new();

        for shape in &self.shapes {
            let schema = strict_object_schema(&shape.schema);
            if schemas.insert(shape.name.clone(), schema).is_some() {
                errors.push(format!("duplicate schema: {}", shape.name));
            }
        }

        for endpoint in &self.endpoints {
            if let Some(request) = &endpoint.request
                && !schemas.contains_key(&request.shape)
            {
                errors.push(format!("missing schema: {}", request.shape));
            }
            if let Some(response) = &endpoint.response
                && !schemas.contains_key(response)
            {
                errors.push(format!("missing schema: {response}"));
            }
            if let Some(query) = &endpoint.query
                && !schemas.contains_key(query)
            {
                errors.push(format!("missing schema: {query}"));
            }
        }

        let known = schemas.keys().collect::<BTreeSet<_>>();
        let mut references = BTreeSet::new();
        for schema in schemas.values() {
            collect_schema_references(schema, &mut references);
        }
        for reference in references {
            if !known.contains(&reference) {
                errors.push(format!("missing schema: {reference}"));
            }
        }

        if errors.is_empty() {
            Ok(schemas)
        } else {
            Err(errors)
        }
    }

    /// Generates a small, dependency-free TypeScript client from this API.
    pub fn typescript(&self) -> Result<String, Vec<String>> {
        let document = self.openapi("Mavi", "0.1.0")?;
        let schemas = document["components"]["schemas"]
            .as_object()
            .ok_or_else(|| vec!["OpenAPI schemas are not an object".to_owned()])?;
        let mut output =
            String::from("// Generated from the canonical Mavi API. Do not edit by hand.\n\n");
        output.push_str("export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };\n\n");

        for (name, schema) in schemas {
            render_typescript_shape(&mut output, name, schema);
            output.push('\n');
        }

        output.push_str(
            "export interface MaviOperation {\n  method: \"get\" | \"post\" | \"put\" | \"patch\" | \"delete\";\n  path: string;\n  input: { location: \"json\" | \"query\" | \"raw\"; shape: string } | null;\n  query: string | null;\n  output: string | null;\n  outputLocation?: \"json\" | \"raw\";\n  status: number;\n  authentication: string;\n  permission: { capability: string; action: string } | null;\n}\n\nexport const operations = {\n",
        );
        for endpoint in &self.endpoints {
            let input = endpoint.request.as_ref().map_or_else(
                || "null".to_owned(),
                |request| {
                    format!(
                        "{{ location: \"{}\", shape: \"{}\" }}",
                        match request.location {
                            InputLocation::Json => "json",
                            InputLocation::Query => "query",
                            InputLocation::Raw => "raw",
                        },
                        request.shape
                    )
                },
            );
            let query = endpoint
                .query
                .as_ref()
                .map_or_else(|| "null".to_owned(), |shape| format!("\"{shape}\""));
            let response = endpoint
                .response
                .as_ref()
                .map_or_else(|| "null".to_owned(), |shape| format!("\"{shape}\""));
            let output_location = match endpoint.response_location {
                OutputLocation::Json => String::new(),
                OutputLocation::Raw => ", outputLocation: \"raw\"".to_owned(),
            };
            let permission = endpoint.permission.as_ref().map_or_else(
                || "null".to_owned(),
                |permission| {
                    format!(
                        "{{ capability: \"{}\", action: \"{}\" }}",
                        permission.capability.as_str(),
                        permission.action.as_str()
                    )
                },
            );
            writeln!(
                output,
                "  \"{}\": {{ method: \"{}\", path: \"{}\", input: {}, query: {}, output: {}{}, status: {}, authentication: \"{}\", permission: {} }},",
                endpoint.operation_id,
                endpoint.method.as_str(),
                endpoint.path,
                input,
                query,
                response,
                output_location,
                endpoint.status,
                authentication_name(endpoint.authentication),
                permission,
            )
            .expect("writing to a String cannot fail");
        }
        output.push_str("} as const satisfies Record<string, MaviOperation>;\n\n");
        output.push_str("export type OperationName = keyof typeof operations;\n\n");
        output.push_str("export interface OperationArguments {\n");
        for endpoint in &self.endpoints {
            render_operation_arguments(&mut output, endpoint);
        }
        output.push_str("}\n\nexport interface OperationResponses {\n");
        for endpoint in &self.endpoints {
            writeln!(
                output,
                "  \"{}\": {};",
                endpoint.operation_id,
                if endpoint.status == 204 {
                    "void".to_owned()
                } else {
                    endpoint
                        .response
                        .as_ref()
                        .map_or_else(|| "void".to_owned(), Clone::clone)
                },
            )
            .expect("writing to a String cannot fail");
        }
        output.push_str("}\n\n");
        output.push_str(TYPESCRIPT_CLIENT);
        Ok(output)
    }

    /// Generates Rust data types and operation metadata for another Mavi
    /// service or an integration test. Transport remains an adapter concern;
    /// this artifact only carries the stable contract vocabulary.
    pub fn rust_client(&self) -> Result<String, Vec<String>> {
        let document = self.openapi("Mavi", "0.1.0")?;
        let schemas = document["components"]["schemas"]
            .as_object()
            .ok_or_else(|| vec!["OpenAPI schemas are not an object".to_owned()])?;
        let mut output = String::from(
            "// Generated from the canonical Mavi API. Do not edit by hand.\n\nuse serde::{Deserialize, Serialize};\nuse serde_json::Value;\n\n",
        );

        for (name, schema) in schemas {
            render_rust_shape(&mut output, name, schema);
            output.push('\n');
        }

        output.push_str(
            "#[derive(Clone, Copy, Debug, Eq, PartialEq)]\npub struct OperationDefinition {\n    pub name: &'static str,\n    pub method: &'static str,\n    pub path: &'static str,\n    pub request: Option<&'static str>,\n    pub request_location: Option<&'static str>,\n    pub query: Option<&'static str>,\n    pub response: Option<&'static str>,\n    pub response_location: Option<&'static str>,\n    pub status: u16,\n    pub authentication: &'static str,\n    pub capability: Option<&'static str>,\n    pub action: Option<&'static str>,\n}\n\npub const OPERATIONS: &[OperationDefinition] = &[\n",
        );
        for endpoint in &self.endpoints {
            let request = endpoint.request.as_ref().map_or_else(
                || "None".to_owned(),
                |request| format!("Some(\"{}\")", request.shape),
            );
            let request_location = endpoint.request.as_ref().map_or_else(
                || "None".to_owned(),
                |request| {
                    format!(
                        "Some(\"{}\")",
                        match request.location {
                            InputLocation::Json => "json",
                            InputLocation::Query => "query",
                            InputLocation::Raw => "raw",
                        }
                    )
                },
            );
            let query = endpoint
                .query
                .as_ref()
                .map_or_else(|| "None".to_owned(), |shape| format!("Some(\"{shape}\")"));
            let response = endpoint
                .response
                .as_ref()
                .map_or_else(|| "None".to_owned(), |shape| format!("Some(\"{shape}\")"));
            let response_location = match endpoint.response_location {
                OutputLocation::Json => "None",
                OutputLocation::Raw => "Some(\"raw\")",
            };
            let (capability, action) = endpoint.permission.as_ref().map_or_else(
                || ("None".to_owned(), "None".to_owned()),
                |permission| {
                    (
                        format!("Some(\"{}\")", permission.capability.as_str()),
                        format!("Some(\"{}\")", permission.action.as_str()),
                    )
                },
            );
            writeln!(
                output,
                "    OperationDefinition {{ name: \"{}\", method: \"{}\", path: \"{}\", request: {}, request_location: {}, query: {}, response: {}, response_location: {}, status: {}, authentication: \"{}\", capability: {}, action: {} }},",
                endpoint.operation_id,
                endpoint.method.as_str(),
                endpoint.path,
                request,
                request_location,
                query,
                response,
                response_location,
                endpoint.status,
                authentication_name(endpoint.authentication),
                capability,
                action,
            )
            .expect("writing to a String cannot fail");
        }
        output.push_str("];\n");
        Ok(output)
    }

    /// Generates MCP tool descriptors only for account/assistant-capable API
    /// operations. Permission metadata remains attached so an MCP adapter can
    /// perform the same Cedar decision as the HTTP adapter.
    pub fn mcp_tools(&self) -> Result<Value, Vec<String>> {
        let schemas = self.schemas()?;
        let mut tools = Vec::new();
        for endpoint in &self.endpoints {
            if !matches!(
                endpoint.authentication,
                Authentication::AccountOrAssistant | Authentication::Assistant
            ) {
                continue;
            }

            let mut properties = Map::new();
            let mut required = Vec::new();
            let path = path_parameters(&endpoint.path);
            if !path.is_empty() {
                let path_properties = path
                    .iter()
                    .map(|name| (name.clone(), json!({"type": "string"})))
                    .collect::<Map<_, _>>();
                properties.insert(
                    "path".to_owned(),
                    json!({"type": "object", "properties": path_properties, "required": path}),
                );
                required.push("path");
            }
            if let Some(request) = &endpoint.request {
                let mut active = BTreeSet::new();
                let schema = schemas
                    .get(&request.shape)
                    .map(|schema| inline_schema(schema, &schemas, &mut active))
                    .ok_or_else(|| vec![format!("missing schema: {}", request.shape)])?;
                properties.insert(
                    match request.location {
                        InputLocation::Json | InputLocation::Raw => "body".to_owned(),
                        InputLocation::Query => "query".to_owned(),
                    },
                    schema,
                );
                required.push(match request.location {
                    InputLocation::Json | InputLocation::Raw => "body",
                    InputLocation::Query => "query",
                });
            }

            if let Some(query) = &endpoint.query {
                let mut active = BTreeSet::new();
                let schema = schemas
                    .get(query)
                    .map(|schema| inline_schema(schema, &schemas, &mut active))
                    .ok_or_else(|| vec![format!("missing schema: {query}")])?;
                properties.insert("query".to_owned(), schema);
                required.push("query");
            }

            tools.push(json!({
                "name": endpoint.operation_id,
                "description": endpoint.summary,
                "inputSchema": {
                    "type": "object",
                    "properties": properties,
                    "required": required,
                    "additionalProperties": false,
                },
                "annotations": {"readOnlyHint": matches!(endpoint.mutation, Mutation::None)},
                "x-mavi": {
                    "authentication": endpoint.authentication,
                    "scope": endpoint.scope,
                    "permission": endpoint.permission,
                },
            }));
        }
        Ok(json!({"tools": tools}))
    }
}

/// Object-shaped contract inputs are closed at the boundary. Domains can
/// still explicitly opt a nested value into arbitrary keys (for example
/// content fields or flow configuration); only the named shape itself gets
/// this default. The HTTP extractor applies the same rule at runtime.
fn strict_object_schema(schema: &Value) -> Value {
    let Value::Object(values) = schema else {
        return schema.clone();
    };
    if values.get("type").and_then(Value::as_str) != Some("object")
        || values.contains_key("additionalProperties")
    {
        return schema.clone();
    }

    let mut strict = values.clone();
    strict.insert("additionalProperties".to_owned(), Value::Bool(false));
    Value::Object(strict)
}

fn query_parameters(api: &Api, shape_name: &str) -> Result<Vec<Value>, Vec<String>> {
    let Some(shape) = api.shapes.iter().find(|shape| shape.name == shape_name) else {
        return Err(vec![format!("missing schema: {shape_name}")]);
    };
    if shape.schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err(vec![format!("query schema is not an object: {shape_name}")]);
    }
    let properties = shape
        .schema
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let required = shape
        .schema
        .get("required")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    Ok(properties
        .iter()
        .map(|(name, schema)| {
            json!({
                "name": name,
                "in": "query",
                "required": required.contains(name.as_str()),
                "schema": schema,
            })
        })
        .collect())
}

fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn inline_schema(
    schema: &Value,
    schemas: &BTreeMap<String, Value>,
    active: &mut BTreeSet<String>,
) -> Value {
    match schema {
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| inline_schema(value, schemas, active))
                .collect(),
        ),
        Value::Object(values) => {
            let mut rewritten = Map::new();
            if let Some(reference) = values.get("$ref").and_then(Value::as_str)
                && let Some(name) = reference.strip_prefix("#/components/schemas/")
            {
                if !active.insert(name.to_owned()) {
                    return json!({"type": "object"});
                }
                let inlined = schemas.get(name).map_or_else(
                    || Value::Object(values.clone()),
                    |value| inline_schema(value, schemas, active),
                );
                active.remove(name);
                return inlined;
            }
            for (key, value) in values {
                let value = inline_schema(value, schemas, active);
                rewritten.insert(key.clone(), value);
            }
            Value::Object(rewritten)
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => schema.clone(),
    }
}

fn collect_schema_references(schema: &Value, references: &mut BTreeSet<String>) {
    match schema {
        Value::Array(values) => {
            for value in values {
                collect_schema_references(value, references);
            }
        }
        Value::Object(values) => {
            if let Some(reference) = values.get("$ref").and_then(Value::as_str)
                && let Some(name) = reference.strip_prefix("#/components/schemas/")
            {
                references.insert(name.to_owned());
            }
            for value in values.values() {
                collect_schema_references(value, references);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn success_response(endpoint: &Endpoint) -> Value {
    if endpoint.status == 204 || endpoint.response.is_none() {
        return json!({"description": "Successful response"});
    }

    let content_type = match endpoint.response_location {
        OutputLocation::Json => "application/json",
        OutputLocation::Raw => "application/octet-stream",
    };
    let content = json!({
        "schema": schema_ref(endpoint.response.as_deref().unwrap_or("Empty")),
    });
    let mut contents = Map::new();
    contents.insert(content_type.to_owned(), content);
    json!({
        "description": "Successful response",
        "content": contents
    })
}

fn error_status(error: ErrorCode) -> u16 {
    match error {
        ErrorCode::Validation => 400,
        ErrorCode::Unauthenticated => 401,
        ErrorCode::Forbidden => 403,
        ErrorCode::NotFound => 404,
        ErrorCode::Conflict => 409,
        ErrorCode::RateLimited => 429,
        ErrorCode::Internal => 500,
    }
}

fn authentication_name(authentication: Authentication) -> &'static str {
    match authentication {
        Authentication::Public => "public",
        Authentication::Account => "account",
        Authentication::AccountOrAssistant => "account_or_assistant",
        Authentication::Student => "student",
        Authentication::Assistant => "assistant",
    }
}

fn path_parameters(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| {
            segment
                .strip_prefix('{')
                .and_then(|value| value.strip_suffix('}'))
        })
        .map(str::to_owned)
        .collect()
}

fn render_typescript_shape(output: &mut String, name: &str, schema: &Value) {
    if schema.get("type").and_then(Value::as_str) == Some("object") {
        let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
            writeln!(output, "export type {name} = Record<string, unknown>;").expect("String");
            return;
        };
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        writeln!(output, "export interface {name} {{").expect("String");
        for (property, property_schema) in properties {
            writeln!(
                output,
                "  {}{}: {};",
                typescript_property(property),
                if required.contains(property.as_str()) {
                    ""
                } else {
                    "?"
                },
                typescript_type(property_schema),
            )
            .expect("String");
        }
        output.push_str("}\n");
    } else {
        writeln!(output, "export type {name} = {};", typescript_type(schema)).expect("String");
    }
}

fn render_rust_shape(output: &mut String, name: &str, schema: &Value) {
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        writeln!(output, "pub type {name} = {};", rust_type(schema)).expect("String");
        return;
    }
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        writeln!(output, "pub type {name} = Value;").expect("String");
        return;
    };
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    writeln!(
        output,
        "#[derive(Clone, Debug, Deserialize, Serialize)]\npub struct {name} {{"
    )
    .expect("String");
    for (property, property_schema) in properties {
        let mut property_type = rust_type(property_schema);
        if !required.contains(property.as_str()) && !property_type.starts_with("Option<") {
            property_type = format!("Option<{property_type}>");
        }
        let field = rust_property(property);
        if field == *property {
            writeln!(output, "    pub {field}: {property_type},").expect("String");
        } else {
            writeln!(
                output,
                "    #[serde(rename = \"{property}\")]\n    pub {field}: {property_type},"
            )
            .expect("String");
        }
    }
    output.push_str("}\n");
}

fn rust_type(schema: &Value) -> String {
    if schema.get("format").and_then(Value::as_str) == Some("binary") {
        return "Vec<u8>".to_owned();
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return reference.rsplit('/').next().unwrap_or("Value").to_owned();
    }
    if schema.get("oneOf").is_some() || schema.get("anyOf").is_some() {
        return "Value".to_owned();
    }
    if let Some(types) = schema.get("type").and_then(Value::as_array) {
        let Some(non_null) = types.iter().find(|value| value.as_str() != Some("null")) else {
            return "Value".to_owned();
        };
        let base = non_null
            .as_str()
            .map_or_else(|| rust_type(non_null), rust_type_name);
        return if types.iter().any(|value| value.as_str() == Some("null")) {
            format!("Option<{base}>")
        } else {
            base
        };
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("array") => format!(
            "Vec<{}>",
            schema
                .get("items")
                .map_or_else(|| "Value".to_owned(), rust_type)
        ),
        Some(kind) => rust_type_name(kind),
        None => "Value".to_owned(),
    }
}

fn rust_type_name(kind: &str) -> String {
    match kind {
        "string" => "String".to_owned(),
        "integer" => "i64".to_owned(),
        "number" => "f64".to_owned(),
        "boolean" => "bool".to_owned(),
        _ => "Value".to_owned(),
    }
}

fn rust_property(property: &str) -> String {
    match property {
        "type" => "type_".to_owned(),
        "self" => "self_".to_owned(),
        "match" => "match_".to_owned(),
        _ => property.replace('-', "_"),
    }
}

fn typescript_property(property: &str) -> String {
    if property.chars().enumerate().all(|(index, character)| {
        character == '_'
            || character == '$'
            || character.is_ascii_alphanumeric()
                && (index > 0
                    || character.is_ascii_alphabetic()
                    || character == '_'
                    || character == '$')
    }) {
        property.to_owned()
    } else {
        format!("\"{property}\"")
    }
}

fn typescript_type(schema: &Value) -> String {
    if schema.get("format").and_then(Value::as_str) == Some("binary") {
        return "Blob".to_owned();
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return reference.rsplit('/').next().unwrap_or("unknown").to_owned();
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return values
            .iter()
            .filter_map(Value::as_str)
            .map(|value| format!("\"{value}\""))
            .collect::<Vec<_>>()
            .join(" | ");
    }
    if let Some(values) = schema.get("oneOf").and_then(Value::as_array) {
        return values
            .iter()
            .map(typescript_type)
            .collect::<Vec<_>>()
            .join(" | ");
    }
    if let Some(types) = schema.get("type").and_then(Value::as_array) {
        return types
            .iter()
            .filter_map(Value::as_str)
            .map(type_name)
            .collect::<Vec<_>>()
            .join(" | ");
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("string") => "string".to_owned(),
        Some("integer" | "number") => "number".to_owned(),
        Some("boolean") => "boolean".to_owned(),
        Some("array") => format!(
            "{}[]",
            schema
                .get("items")
                .map_or_else(|| "unknown".to_owned(), typescript_type)
        ),
        Some("object") => "Record<string, unknown>".to_owned(),
        _ => "unknown".to_owned(),
    }
}

fn type_name(value: &str) -> String {
    match value {
        "string" => "string".to_owned(),
        "integer" | "number" => "number".to_owned(),
        "boolean" => "boolean".to_owned(),
        "null" => "null".to_owned(),
        "object" => "Record<string, unknown>".to_owned(),
        "array" => "unknown[]".to_owned(),
        _ => "unknown".to_owned(),
    }
}

fn render_operation_arguments(output: &mut String, endpoint: &Endpoint) {
    let parameters = path_parameters(&endpoint.path);
    write!(output, "  \"{}\": {{", endpoint.operation_id).expect("String");
    if parameters.is_empty() {
        output.push_str(" path?: never;");
    } else {
        write!(
            output,
            " path: {{ {} }};",
            parameters
                .iter()
                .map(|name| format!("{name}: string"))
                .collect::<Vec<_>>()
                .join("; ")
        )
        .expect("String");
    }
    let query_shape = endpoint.query.as_deref().or_else(|| {
        endpoint.request.as_ref().and_then(|request| {
            (request.location == InputLocation::Query).then_some(request.shape.as_str())
        })
    });
    match query_shape {
        Some(shape) => write!(output, " query: {shape};").expect("String"),
        None => output.push_str(" query?: never;"),
    }
    match endpoint.request.as_ref().map(|request| request.location) {
        Some(InputLocation::Json) => write!(
            output,
            " body: {};",
            endpoint
                .request
                .as_ref()
                .map_or("unknown", |request| request.shape.as_str())
        )
        .expect("String"),
        Some(InputLocation::Raw) => output.push_str(" body: Blob | ArrayBuffer | Uint8Array;"),
        Some(InputLocation::Query) | None => output.push_str(" body?: never;"),
    }
    output.push_str(" }\n");
}

const TYPESCRIPT_CLIENT: &str = r#"export interface MaviClientOptions {
  baseUrl: string;
  token?: string;
  fetch?: typeof globalThis.fetch;
}

export class MaviApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly payload: ErrorEnvelope | null,
  ) {
    super(payload?.error.message ?? `Mavi request failed with status ${status}`);
  }
}

export class MaviClient {
  private readonly baseUrl: string;
  private readonly fetcher: typeof globalThis.fetch;

  constructor(private readonly options: MaviClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, "");
    this.fetcher = options.fetch ?? globalThis.fetch;
  }

  async call<Name extends OperationName>(
    operation: Name,
    args: OperationArguments[Name],
  ): Promise<OperationResponses[Name]> {
    const definition = operations[operation];
    const values = args as {
      path?: Record<string, string>;
      query?: Record<string, unknown>;
      body?: unknown;
    };
    let path: string = definition.path;
    for (const [name, value] of Object.entries(values.path ?? {})) {
      path = path.replace(`{${name}}`, encodeURIComponent(value));
    }
    const url = new URL(`${this.baseUrl}${path}`);
    for (const [name, value] of Object.entries(values.query ?? {})) {
      if (value !== undefined && value !== null) {
        url.searchParams.set(name, String(value));
      }
    }
    const rawResponse = definition.outputLocation === "raw";
    const headers: Record<string, string> = {
      Accept: rawResponse ? "application/octet-stream" : "application/json",
    };
    if (this.options.token) {
      headers.Authorization = `Bearer ${this.options.token}`;
    }
    const rawBody = definition.input?.location === "raw";
    let requestBody: BodyInit | undefined;
    if (values.body !== undefined) {
      headers["Content-Type"] = rawBody
        ? "application/octet-stream"
        : "application/json";
      requestBody = rawBody
        ? (values.body as BodyInit)
        : JSON.stringify(values.body);
    }
    const response = await this.fetcher(url, {
      method: definition.method.toUpperCase(),
      headers,
      body: requestBody,
    });
    if (!response.ok) {
      let payload: ErrorEnvelope | null = null;
      try {
        payload = (await response.json()) as ErrorEnvelope;
      } catch {
        payload = null;
      }
      throw new MaviApiError(response.status, payload);
    }
    if (response.status === 204) {
      return undefined as OperationResponses[Name];
    }
    return (rawResponse
      ? await response.blob()
      : await response.json()) as OperationResponses[Name];
  }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_core::{Action, Capability};

    #[test]
    fn duplicate_routes_are_rejected() {
        let api = Api::new([
            Endpoint::new(Method::Get, "/api/v1/health", "health.read", "Health"),
            Endpoint::new(Method::Get, "/api/v1/health", "health.read_again", "Health"),
        ]);

        let errors = api.validate().expect_err("duplicate route must fail");
        assert!(errors.iter().any(|error| error.contains("duplicate route")));
    }

    #[test]
    fn mutations_require_a_permission() {
        let api = Api::new([Endpoint::new(
            Method::Post,
            "/api/v1/content",
            "content.create",
            "Create content",
        )
        .changes(true)]);

        assert!(api.validate().is_err());

        let api = Api::new([Endpoint::new(
            Method::Post,
            "/api/v1/content",
            "content.create",
            "Create content",
        )
        .requires(Permission {
            capability: Capability::Content,
            action: Action::Write,
        })
        .changes(true)]);

        assert!(api.validate().is_ok());
    }

    #[test]
    fn control_plane_endpoints_cannot_be_public() {
        let api = Api::new([Endpoint::new(
            Method::Get,
            "/operator/v1/sites",
            "operator.sites.list",
            "List sites",
        )
        .public()
        .control_plane()]);

        assert!(api.validate().is_err());
    }

    #[test]
    fn public_mutations_are_explicit() {
        let api = Api::new([Endpoint::new(
            Method::Post,
            "/api/v1/auth/sessions",
            "auth.session.create",
            "Create a session",
        )
        .public_mutation()]);

        assert!(api.validate().is_ok());
    }

    #[test]
    fn assistant_capable_endpoints_are_explicit() {
        let api = Api::new([Endpoint::new(
            Method::Get,
            "/api/v1/content",
            "content.list",
            "List content",
        )
        .account_or_assistant()]);

        assert_eq!(
            api.endpoints[0].authentication,
            Authentication::AccountOrAssistant
        );
        assert!(api.validate().is_ok());
    }

    #[test]
    fn openapi_is_generated_from_the_same_endpoint_declaration() {
        let api = Api::new([
            Endpoint::new(Method::Get, "/api/v1/health", "health.read", "Health")
                .public()
                .returns(200, "Health"),
        ])
        .with_shapes([Shape::new("Health", json!({"type": "string"}))]);
        let document = api.openapi("Mavi", "0.1.0").expect("OpenAPI");

        assert_eq!(document["openapi"], "3.1.0");
        assert_eq!(
            document["paths"]["/api/v1/health"]["get"]["operationId"],
            "health.read"
        );
    }

    #[test]
    fn query_inputs_become_parameters_and_keep_cursor_contract() {
        let api =
            Api::new([
                Endpoint::new(Method::Get, "/api/v1/people", "people.list", "List people")
                    .public()
                    .takes_query("PeopleListFilter")
                    .returns(200, "PeoplePage"),
            ])
            .with_shapes([
                Shape::new(
                    "PeopleListFilter",
                    json!({
                        "type": "object",
                        "properties": {
                            "after": {"type": ["string", "null"]},
                            "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                        }
                    }),
                ),
                Shape::new("PeoplePage", json!({"type": "object"})),
            ]);

        let document = api.openapi("Mavi", "0.1.0").expect("OpenAPI");
        let operation = &document["paths"]["/api/v1/people"]["get"];
        assert!(operation.get("requestBody").is_none());
        assert_eq!(operation["parameters"][0]["name"], "after");
        assert_eq!(operation["parameters"][1]["name"], "limit");
        assert_eq!(
            document["components"]["schemas"]["PeopleListFilter"]["additionalProperties"],
            false
        );
    }

    #[test]
    fn fingerprint_is_deterministic_and_changes_with_the_contract() {
        let first = Api::new([Endpoint::new(
            Method::Get,
            "/api/v1/health",
            "health.read",
            "Health",
        )]);
        let second = Api::new([Endpoint::new(
            Method::Get,
            "/api/v1/health",
            "health.read",
            "Health",
        )]);
        let changed = Api::new([Endpoint::new(
            Method::Get,
            "/api/v1/runtime/manifest",
            "runtime.manifest.read",
            "Runtime manifest",
        )]);

        assert_eq!(
            first.fingerprint().expect("fingerprint"),
            second.fingerprint().expect("fingerprint")
        );
        assert_ne!(
            first.fingerprint().expect("fingerprint"),
            changed.fingerprint().expect("fingerprint")
        );
        assert!(
            first
                .fingerprint()
                .expect("fingerprint")
                .starts_with("sha256:")
        );
    }

    #[test]
    fn raw_inputs_generate_binary_body_and_independent_query_parameters() {
        let api = Api::new([Endpoint::new(
            Method::Post,
            "/api/v1/files",
            "media.files.upload",
            "Upload a file",
        )
        .account_or_assistant()
        .takes_raw("FileBytes")
        .with_query("UploadFileQuery")
        .returns(201, "File")])
        .with_shapes([
            Shape::new("FileBytes", json!({"type": "string", "format": "binary"})),
            Shape::new(
                "UploadFileQuery",
                json!({
                    "type": "object",
                    "required": ["name"],
                    "properties": {"name": {"type": "string"}},
                }),
            ),
            Shape::new("File", json!({"type": "object"})),
        ]);

        let document = api.openapi("Mavi", "0.1.0").expect("OpenAPI");
        let operation = &document["paths"]["/api/v1/files"]["post"];
        assert_eq!(
            operation["requestBody"]["content"]["application/octet-stream"]["schema"]["$ref"],
            "#/components/schemas/FileBytes"
        );
        assert_eq!(operation["parameters"][0]["name"], "name");
        assert!(
            api.typescript()
                .expect("TypeScript")
                .contains("location: \"raw\"")
        );
    }

    #[test]
    fn strict_top_level_shapes_keep_nested_open_maps_open() {
        let api = Api::new([Endpoint::new(
            Method::Post,
            "/api/v1/content",
            "content.create",
            "Create content",
        )
        .public_mutation()
        .takes("CreateContent")
        .returns(201, "Content")])
        .with_shapes([
            Shape::new(
                "CreateContent",
                json!({
                    "type": "object",
                    "properties": {
                        "fields": {"type": "object", "additionalProperties": true}
                    }
                }),
            ),
            Shape::new("Content", json!({"type": "object"})),
        ]);

        let document = api.openapi("Mavi", "0.1.0").expect("OpenAPI");
        assert_eq!(
            document["components"]["schemas"]["CreateContent"]["additionalProperties"],
            false
        );
        assert_eq!(
            document["components"]["schemas"]["CreateContent"]["properties"]["fields"]["additionalProperties"],
            true
        );
    }

    #[test]
    fn raw_outputs_generate_binary_response_and_binary_client_types() {
        let api = Api::new([Endpoint::new(
            Method::Get,
            "/student/v1/learning/media/{id}",
            "learning.media.read",
            "Read protected media",
        )
        .student()
        .returns_raw(200, "FileBytes")])
        .with_shapes([Shape::new(
            "FileBytes",
            json!({"type": "string", "format": "binary"}),
        )]);

        let document = api.openapi("Mavi", "0.1.0").expect("OpenAPI");
        let operation = &document["paths"]["/student/v1/learning/media/{id}"]["get"];
        assert!(
            operation["responses"]["200"]["content"]
                .get("application/octet-stream")
                .is_some()
        );
        let typescript = api.typescript().expect("TypeScript");
        assert!(typescript.contains("export type FileBytes = Blob;"));
        assert!(typescript.contains("outputLocation: \"raw\""));
    }

    #[test]
    fn generated_client_and_mcp_tools_share_permission_metadata() {
        let api =
            Api::new([
                Endpoint::new(Method::Get, "/api/v1/people", "people.list", "List people")
                    .account_or_assistant()
                    .requires(Permission {
                        capability: Capability::People,
                        action: Action::View,
                    })
                    .takes_query("PeopleListFilter")
                    .returns(200, "PeoplePage"),
            ])
            .with_shapes([
                Shape::new("PeopleListFilter", json!({"type": "object"})),
                Shape::new("PeoplePage", json!({"type": "object"})),
            ]);

        let typescript = api.typescript().expect("TypeScript");
        assert!(typescript.contains("people.list"));
        assert!(typescript.contains("query: PeopleListFilter"));

        let tools = api.mcp_tools().expect("MCP tools");
        assert_eq!(tools["tools"][0]["name"], "people.list");
        assert_eq!(tools["tools"][0]["x-mavi"]["permission"]["action"], "view");
        assert_eq!(
            tools["tools"][0]["inputSchema"]["properties"]["query"]["type"],
            "object"
        );
        assert!(tools["tools"][0]["inputSchema"]["properties"]["query"]["$ref"].is_null());
    }

    #[test]
    fn openapi_rejects_a_nested_reference_to_an_undefined_shape() {
        let api = Api::new([
            Endpoint::new(Method::Get, "/api/v1/health", "health.read", "Health")
                .public()
                .returns(200, "Health"),
        ])
        .with_shapes([Shape::new(
            "Health",
            json!({"$ref": "#/components/schemas/DoesNotExist"}),
        )]);

        let errors = api.openapi("Mavi", "0.1.0").expect_err("missing ref");
        assert!(errors.iter().any(|error| error.contains("DoesNotExist")));
    }
}
