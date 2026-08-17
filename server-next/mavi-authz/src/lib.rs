//! Cedar authorization for site-scoped Mavi operations.
//!
//! Cedar is embedded in both self-host and cloud runtimes. The operator never
//! becomes an authorization dependency: it may provision a site, while Mavi
//! evaluates site permissions from the authenticated principal, grants and
//! request context.

use std::str::FromStr;

use cedar_policy::{
    Authorizer, Context, Decision, Entities, EntityUid, PolicySet, Request, Schema, ValidationMode,
    Validator,
};
use mavi_core::{Caller, Grant, Grants, MaviError, SiteContext, SiteId};
use serde_json::json;

const SITE_POLICY: &str = include_str!("../policies/site.cedar");
const SITE_SCHEMA: &str = include_str!("../policies/site.cedarschema");

#[derive(Clone, Debug)]
pub struct AuthorizationRequest {
    pub principal_id: String,
    pub principal_site_id: SiteId,
    pub grants: Grants,
    pub grant: Grant,
    pub resource_type: String,
    pub resource_id: String,
    pub resource_site_id: SiteId,
    pub request_site_id: SiteId,
}

#[derive(Clone, Debug)]
pub struct CedarAuthorizer {
    authorizer: Authorizer,
    policies: PolicySet,
}

impl CedarAuthorizer {
    pub fn new() -> Result<Self, MaviError> {
        let policies = PolicySet::from_str(SITE_POLICY).map_err(|_| MaviError::Internal)?;
        let (schema, _) =
            Schema::from_cedarschema_str(SITE_SCHEMA).map_err(|_| MaviError::Internal)?;
        if !Validator::new(schema)
            .validate(&policies, ValidationMode::Strict)
            .validation_passed()
        {
            return Err(MaviError::Internal);
        }
        Ok(Self {
            authorizer: Authorizer::new(),
            policies,
        })
    }

    pub fn authorize(&self, request: &AuthorizationRequest) -> Result<(), MaviError> {
        if request.principal_site_id != request.request_site_id
            || request.resource_site_id != request.request_site_id
        {
            return Err(MaviError::Forbidden);
        }

        let principal = uid("Principal", &request.principal_id)?;
        let action = uid("Action", "authorize")?;
        let resource = uid("Resource", &request.resource_id)?;
        let context = Context::from_json_value(
            json!({
                "site_id": request.request_site_id.to_string(),
                "grant": format!("{}:{}", request.grant.capability.as_str(), request.grant.action.as_str()),
            }),
            None,
        )
        .map_err(|_| MaviError::Internal)?;
        let cedar_request = Request::new(principal, action, resource, context, None)
            .map_err(|_| MaviError::Internal)?;
        let entities = Entities::from_json_value(
            json!([
                {
                    "uid": {"type": "Principal", "id": request.principal_id},
                    "attrs": {
                        "site_id": request.principal_site_id.to_string(),
                        "grants": request.grants.as_slice().iter().map(|grant| format!("{}:{}", grant.capability.as_str(), grant.action.as_str())).collect::<Vec<_>>(),
                    },
                    "parents": []
                },
                {
                    "uid": {"type": "Resource", "id": request.resource_id},
                    "attrs": {
                        "site_id": request.resource_site_id.to_string(),
                        "kind": request.resource_type
                    },
                    "parents": []
                }
            ]),
            None,
        )
        .map_err(|_| MaviError::Internal)?;

        if self
            .authorizer
            .is_authorized(&cedar_request, &self.policies, &entities)
            .decision()
            == Decision::Allow
        {
            Ok(())
        } else {
            Err(MaviError::Forbidden)
        }
    }

    pub fn authorize_context(
        &self,
        context: &SiteContext,
        grant: Grant,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        resource_site_id: SiteId,
    ) -> Result<(), MaviError> {
        let (principal_id, grants) = match &context.caller {
            Caller::Account {
                person_id, grants, ..
            } => (person_id.to_string(), grants.clone()),
            Caller::Assistant { key_id, grants, .. } => (key_id.to_string(), grants.clone()),
            Caller::Public | Caller::Student { .. } => return Err(MaviError::Forbidden),
        };

        self.authorize(&AuthorizationRequest {
            principal_id,
            principal_site_id: context.site_id,
            grants,
            grant,
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            resource_site_id,
            request_site_id: context.site_id,
        })
    }
}

fn uid(entity_type: &str, id: &str) -> Result<EntityUid, MaviError> {
    format!("{entity_type}::{id:?}")
        .parse()
        .map_err(|_| MaviError::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_core::{Action, Capability, Grant, Grants};

    fn request(grants: Grants, grant: Grant) -> AuthorizationRequest {
        let site_id = SiteId::new();
        AuthorizationRequest {
            principal_id: "person".to_owned(),
            principal_site_id: site_id,
            grants,
            grant,
            resource_type: "Content".to_owned(),
            resource_id: "post".to_owned(),
            resource_site_id: site_id,
            request_site_id: site_id,
        }
    }

    #[test]
    fn cedar_allows_a_matching_site_grant() {
        let authorizer = CedarAuthorizer::new().expect("policy");
        let grant = Grant::new(Capability::Content, Action::Write);
        assert!(
            authorizer
                .authorize(&request(Grants::new([grant]), grant))
                .is_ok()
        );
    }

    #[test]
    fn cedar_denies_the_wrong_grant_and_cross_site_scope() {
        let authorizer = CedarAuthorizer::new().expect("policy");
        let needed = Grant::new(Capability::Content, Action::Write);
        let held = Grant::new(Capability::Content, Action::View);
        assert!(
            authorizer
                .authorize(&request(Grants::new([held]), needed))
                .is_err()
        );

        let mut cross_site = request(Grants::new([needed]), needed);
        cross_site.resource_site_id = SiteId::new();
        assert!(authorizer.authorize(&cross_site).is_err());
    }
}
