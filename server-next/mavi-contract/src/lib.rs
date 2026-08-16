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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Authentication {
    Public,
    Account,
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
    pub changes: bool,
    pub idempotent: bool,
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
            changes: false,
            idempotent: false,
        }
    }

    #[must_use]
    pub const fn public(mut self) -> Self {
        self.authentication = Authentication::Public;
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
        self.changes = true;
        self.idempotent = idempotent;
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

            if endpoint.changes && endpoint.permission.is_none() {
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
}
