//! Canonical API declarations.
//!
//! A route is not complete unless it says what it takes, returns, refuses,
//! who may call it and whether it requires a site scope. `OpenAPI` and generated
//! clients will be built from this list rather than maintained beside it.

use mavi_core::{Action, Capability, ErrorCode};
use serde::{Deserialize, Serialize};

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
    pub request: Option<String>,
    pub response: Option<String>,
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
            response: None,
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
    pub const fn public_mutation(mut self) -> Self {
        self.authentication = Authentication::Public;
        self.mutation = Mutation::Public { idempotent: false };
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
        self.request = Some(shape.into());
        self
    }

    #[must_use]
    pub fn returns(mut self, status: u16, shape: impl Into<String>) -> Self {
        self.status = status;
        self.response = Some(shape.into());
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

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Api {
    pub endpoints: Vec<Endpoint>,
}

impl Api {
    #[must_use]
    pub fn new(endpoints: impl IntoIterator<Item = Endpoint>) -> Self {
        Self {
            endpoints: endpoints.into_iter().collect(),
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
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn as_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    pub fn openapi(
        &self,
        title: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<serde_json::Value, Vec<String>> {
        self.validate()?;

        let mut paths = serde_json::Map::new();
        for endpoint in &self.endpoints {
            let path = paths
                .entry(endpoint.path.clone())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            let Some(path_item) = path.as_object_mut() else {
                return Err(vec![format!("invalid generated path: {}", endpoint.path)]);
            };

            let mut operation = serde_json::Map::new();
            operation.insert(
                "operationId".to_owned(),
                serde_json::Value::String(endpoint.operation_id.clone()),
            );
            operation.insert(
                "summary".to_owned(),
                serde_json::Value::String(endpoint.summary.clone()),
            );
            operation.insert(
                "responses".to_owned(),
                serde_json::json!({
                    endpoint.status.to_string(): {
                        "description": "Successful response",
                        "content": {
                            "application/json": {
                                "schema": endpoint.response.as_ref().map_or_else(
                                    || serde_json::json!({}),
                                    |shape| serde_json::json!({"$ref": format!("#/components/schemas/{shape}")}),
                                )
                            }
                        }
                    }
                }),
            );
            if let Some(shape) = &endpoint.request {
                operation.insert(
                    "requestBody".to_owned(),
                    serde_json::json!({
                        "required": true,
                        "content": {"application/json": {"schema": {"$ref": format!("#/components/schemas/{shape}")}}}
                    }),
                );
            }
            if endpoint.authentication != Authentication::Public {
                operation.insert(
                    "security".to_owned(),
                    serde_json::json!([{ "bearerAuth": [] }]),
                );
            }
            operation.insert(
                "x-mavi".to_owned(),
                serde_json::json!({
                    "scope": endpoint.scope,
                    "mutation": endpoint.mutation,
                    "permission": endpoint.permission,
                    "errors": endpoint.errors,
                }),
            );
            path_item.insert(
                endpoint.method.as_str().to_owned(),
                serde_json::Value::Object(operation),
            );
        }

        Ok(serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": title.into(), "version": version.into()},
            "paths": paths,
            "components": {"securitySchemes": {"bearerAuth": {"type": "http", "scheme": "bearer"}}}
        }))
    }
}

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
        ]);
        let document = api.openapi("Mavi", "0.1.0").expect("OpenAPI");

        assert_eq!(document["openapi"], "3.1.0");
        assert_eq!(
            document["paths"]["/api/v1/health"]["get"]["operationId"],
            "health.read"
        );
    }
}
