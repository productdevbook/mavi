//! Cedar authorization for site-scoped Mavi operations.
//!
//! Cedar is embedded in both self-host and cloud runtimes. The operator never
//! becomes an authorization dependency: it may provision a site, while Mavi
//! evaluates site permissions from the authenticated principal, grants and
//! request context.

use std::{collections::BTreeSet, str::FromStr};

use cedar_policy::{
    Authorizer, Context, Decision, Entities, EntityUid, PolicySet, Request, Schema, ValidationMode,
    Validator,
};
use mavi_core::{Caller, Grant, Grants, MaviError, Permission, PluginId, SiteContext, SiteId};
use serde_json::{Value, json};

const SITE_POLICY: &str = include_str!("../policies/site.cedar");
const SITE_SCHEMA: &str = include_str!("../policies/site.cedarschema");

#[derive(Clone, Debug)]
pub struct AuthorizationRequest {
    pub principal_id: String,
    pub principal_site_id: SiteId,
    pub grants: Grants,
    /// Grants attached to this concrete resource rather than to the whole
    /// site. The application layer loads these inside the same scoped
    /// transaction as the mutation they protect.
    pub resource_grants: Grants,
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

#[derive(Clone, Debug)]
enum TypedPrincipal {
    Account { id: String, person_id: String },
    Assistant { id: String, key_id: String },
}

impl CedarAuthorizer {
    pub fn new() -> Result<Self, MaviError> {
        Self::from_policy_source(SITE_POLICY)
    }

    /// Builds the embedded policy set from the base site policy and compiled
    /// plugin fragments. Fragments are code-owned strings; no policy text is
    /// accepted from the database or a request.
    pub fn new_with_policy_fragments<'a>(
        fragments: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, MaviError> {
        let mut source = SITE_POLICY.to_owned();
        for fragment in fragments {
            source.push('\n');
            source.push_str(fragment);
        }
        Self::from_policy_source(&source)
    }

    fn from_policy_source(source: &str) -> Result<Self, MaviError> {
        let policies = PolicySet::from_str(source).map_err(|_| MaviError::Internal)?;
        let (schema, _) =
            Schema::from_cedarschema_str(SITE_SCHEMA).map_err(|_| MaviError::Internal)?;
        let validation = Validator::new(schema).validate(&policies, ValidationMode::Strict);
        if !validation.validation_passed() {
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
        let action = uid("Action", &request.grant.permission_key())?;
        let resource = uid("Resource", &request.resource_id)?;
        let plugin = request.grant.capability.plugin();
        let context = Context::from_json_value(
            json!({
                "site_id": request.request_site_id.to_string(),
                "plugin": plugin.as_str(),
                "permission": request.grant.permission_key(),
                "resource_type": request.resource_type,
            }),
            None,
        )
        .map_err(|_| MaviError::Internal)?;
        let cedar_request = Request::new(principal, action, resource, context, None)
            .map_err(|_| MaviError::Internal)?;
        let mut principal_plugins = request
            .grants
            .as_slice()
            .iter()
            .chain(request.resource_grants.as_slice().iter())
            .map(|grant| grant.capability.plugin().as_str().to_owned())
            .collect::<Vec<_>>();
        principal_plugins.sort_unstable();
        principal_plugins.dedup();
        let entities = cedar_entities(
            &request.principal_id,
            request.principal_site_id,
            &request.grants,
            &request
                .grants
                .as_slice()
                .iter()
                .map(|grant| grant.permission_key())
                .collect::<Vec<_>>(),
            &principal_plugins,
            &request.resource_id,
            request.resource_site_id,
            &request.resource_type,
            &request.resource_grants,
            &request
                .resource_grants
                .as_slice()
                .iter()
                .map(|grant| grant.permission_key())
                .collect::<Vec<_>>(),
            plugin,
            &principal_plugins,
            None,
        )?;

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

    /// Evaluates a namespaced business permission. The only caller of this
    /// method in the application is the use-case layer; HTTP handlers should
    /// not inspect grants directly.
    pub fn authorize_permission(
        &self,
        context: &SiteContext,
        permission: &Permission,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        resource_site_id: SiteId,
        active_plugins: &BTreeSet<PluginId>,
    ) -> Result<(), MaviError> {
        self.authorize_permission_with_resource_grants(
            context,
            permission,
            resource_type,
            resource_id,
            resource_site_id,
            active_plugins,
            &Grants::default(),
        )
    }

    // Keep the authorization port explicit: each value is part of the
    // fail-closed site/resource boundary and hiding them in an untyped tuple
    // would make accidental cross-site calls easier.
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::too_many_arguments)]
    pub fn authorize_permission_with_resource_grants(
        &self,
        context: &SiteContext,
        permission: &Permission,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        resource_site_id: SiteId,
        active_plugins: &BTreeSet<PluginId>,
        resource_grants: &Grants,
    ) -> Result<(), MaviError> {
        if !permission.is_valid()
            || !permission
                .plugin
                .business_actions()
                .contains(&permission.action.as_str())
            || context.site_id != resource_site_id
            || !active_plugins.contains(&permission.plugin)
        {
            return Err(MaviError::Forbidden);
        }

        let (principal_id, grants, typed_principal) = match &context.caller {
            Caller::Account {
                person_id, grants, ..
            } => (
                person_id.to_string(),
                grants.clone(),
                Some(TypedPrincipal::Account {
                    id: person_id.to_string(),
                    person_id: person_id.to_string(),
                }),
            ),
            Caller::Assistant { key_id, grants, .. } => (
                key_id.to_string(),
                grants.clone(),
                Some(TypedPrincipal::Assistant {
                    id: key_id.to_string(),
                    key_id: key_id.to_string(),
                }),
            ),
            Caller::Public => return Err(MaviError::Unauthenticated),
            Caller::Student { .. } | Caller::System { .. } => return Err(MaviError::Forbidden),
        };
        let resource_type = resource_type.into();
        if permission
            .resource_type
            .as_deref()
            .is_some_and(|expected| expected != resource_type)
        {
            return Err(MaviError::Forbidden);
        }
        let resource_id = resource_id.into();
        let mut permissions = grants
            .permissions()
            .iter()
            .filter(|held| permission_applies(held, permission, &resource_type))
            .map(Permission::capability_key)
            .collect::<Vec<_>>();
        // Role rows written before the namespaced permission migration still
        // carry a capability/action pair. Translate those rows into the
        // stable business actions at the Cedar boundary; the database does
        // not get to invent action names.
        for grant in grants.as_slice() {
            if grants
                .permissions()
                .iter()
                .any(|held| held.resource_type.is_none() && held.to_legacy() == Some(*grant))
            {
                permissions.extend(legacy_business_aliases(*grant));
            }
        }
        // Plugin lifecycle is an owner-level core operation. Until the
        // legacy role table is migrated to fully namespaced permissions, the
        // protected People:Write grant is its compatibility representation.
        if grants.as_slice().contains(&Grant::new(
            mavi_core::Capability::People,
            mavi_core::Action::Write,
        )) && grants.permissions().iter().any(|permission| {
            permission.resource_type.is_none()
                && permission.to_legacy()
                    == Some(Grant::new(
                        mavi_core::Capability::People,
                        mavi_core::Action::Write,
                    ))
        }) {
            permissions.push("core.plugins.activate".to_owned());
            permissions.push("core.plugins.deactivate".to_owned());
        }
        if grants.as_slice().contains(&Grant::new(
            mavi_core::Capability::People,
            mavi_core::Action::View,
        )) && grants.permissions().iter().any(|permission| {
            permission.resource_type.is_none()
                && permission.to_legacy()
                    == Some(Grant::new(
                        mavi_core::Capability::People,
                        mavi_core::Action::View,
                    ))
        }) {
            permissions.push("core.plugins.list".to_owned());
        }
        permissions.sort_unstable();
        permissions.dedup();

        let mut plugins = active_plugins
            .iter()
            .map(|plugin| plugin.as_str().to_owned())
            .collect::<Vec<_>>();
        plugins.sort_unstable();
        let mut resource_permissions = resource_grants
            .permissions()
            .iter()
            .filter(|held| permission_applies(held, permission, &resource_type))
            .map(Permission::capability_key)
            .collect::<Vec<_>>();
        for grant in resource_grants.as_slice() {
            if resource_grants
                .permissions()
                .iter()
                .any(|held| held.resource_type.is_none() && held.to_legacy() == Some(*grant))
            {
                resource_permissions.extend(legacy_business_aliases(*grant));
            }
        }
        resource_permissions.sort_unstable();
        resource_permissions.dedup();
        let expected_resource_type = permission
            .resource_type
            .as_deref()
            .unwrap_or("*")
            .to_owned();
        let action = uid("Action", permission.action.as_str())?;
        let principal = uid("Principal", &principal_id)?;
        let resource = uid("Resource", &resource_id)?;
        let context_value = Context::from_json_value(
            json!({
                "site_id": context.site_id.to_string(),
                "plugin": permission.plugin.as_str(),
                "permission": permission.capability_key(),
                "resource_type": expected_resource_type,
            }),
            None,
        )
        .map_err(|_| MaviError::Internal)?;
        let request = Request::new(principal, action, resource, context_value, None)
            .map_err(|_| MaviError::Internal)?;
        let entities = cedar_entities(
            &principal_id,
            context.site_id,
            &grants,
            &permissions,
            &plugins,
            &resource_id,
            resource_site_id,
            &resource_type,
            resource_grants,
            &resource_permissions,
            permission.plugin,
            &plugins,
            typed_principal,
        )?;

        if self
            .authorizer
            .is_authorized(&request, &self.policies, &entities)
            .decision()
            == Decision::Allow
        {
            Ok(())
        } else {
            Err(MaviError::Forbidden)
        }
    }

    /// Migrates a transport-level capability/action pair into the typed
    /// business-action evaluator. This is the only compatibility bridge for
    /// handlers that have not yet changed their endpoint declaration; the
    /// final Cedar decision still uses the namespaced action catalog and the
    /// active-plugin set.
    #[allow(clippy::too_many_arguments)]
    pub fn authorize_legacy_grant(
        &self,
        context: &SiteContext,
        grant: Grant,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        resource_site_id: SiteId,
        active_plugins: &BTreeSet<PluginId>,
        resource_grants: &Grants,
    ) -> Result<(), MaviError> {
        self.authorize_permission_with_resource_grants(
            context,
            &Permission::from_legacy(grant.capability, grant.action),
            resource_type,
            resource_id,
            resource_site_id,
            active_plugins,
            resource_grants,
        )
    }

    pub fn authorize_context(
        &self,
        context: &SiteContext,
        grant: Grant,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        resource_site_id: SiteId,
    ) -> Result<(), MaviError> {
        self.authorize_context_with_resource_grants(
            context,
            grant,
            resource_type,
            resource_id,
            resource_site_id,
            Grants::default(),
        )
    }

    pub fn authorize_context_with_resource_grants(
        &self,
        context: &SiteContext,
        grant: Grant,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        resource_site_id: SiteId,
        resource_grants: Grants,
    ) -> Result<(), MaviError> {
        let (principal_id, grants) = match &context.caller {
            Caller::Account {
                person_id, grants, ..
            } => (person_id.to_string(), grants.clone()),
            Caller::Assistant { key_id, grants, .. } => (key_id.to_string(), grants.clone()),
            Caller::Public => return Err(MaviError::Unauthenticated),
            Caller::Student { .. } | Caller::System { .. } => return Err(MaviError::Forbidden),
        };

        self.authorize(&AuthorizationRequest {
            principal_id,
            principal_site_id: context.site_id,
            grants,
            resource_grants,
            grant,
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            resource_site_id,
            request_site_id: context.site_id,
        })
    }
}

/// Materialize the complete site-scoped entity graph used by Cedar.
///
/// `Principal` and `Resource` remain the stable request-facing entities so
/// existing policies and integrations keep working. Typed identity entities,
/// role projections, the site, and the plugin resource are added as parents so
/// plugin policies can make decisions against the same model without reaching
/// into Rust grant collections. Every entity receives the request site as an
/// attribute or ancestor; a caller can therefore never inherit a grant from a
/// different site by accident.
#[allow(clippy::too_many_arguments)]
fn cedar_entities(
    principal_id: &str,
    principal_site_id: SiteId,
    grants: &Grants,
    permissions: &[String],
    principal_plugins: &[String],
    resource_id: &str,
    resource_site_id: SiteId,
    resource_type: &str,
    resource_grants: &Grants,
    resource_permissions: &[String],
    plugin: PluginId,
    active_plugins: &[String],
    typed_principal: Option<TypedPrincipal>,
) -> Result<Entities, MaviError> {
    let site_id = principal_site_id.to_string();
    let resource_site_id_string = resource_site_id.to_string();
    let plugin_id = plugin.as_str().to_owned();
    let mut entities = Vec::<Value>::new();

    entities.push(json!({
        "uid": {"type": "Site", "id": site_id},
        "attrs": {
            "site_id": principal_site_id.to_string(),
            "active_plugins": active_plugins,
        },
        "parents": []
    }));
    entities.push(json!({
        "uid": {"type": "PluginResource", "id": plugin_id},
        "attrs": {
            "site_id": principal_site_id.to_string(),
            "kind": "Plugin",
            "grants": [],
            "permissions": [],
            "plugins": [plugin.as_str()],
            "plugin_id": plugin.as_str(),
        },
        "parents": [{"type": "Site", "id": principal_site_id.to_string()}]
    }));

    let mut role_ids = Vec::new();
    for (index, permission) in permissions.iter().enumerate() {
        let role_id = format!("mavi-role-{principal_id}-{index}");
        role_ids.push(role_id.clone());
        entities.push(json!({
            "uid": {"type": "Role", "id": role_id},
            "attrs": {
                "site_id": principal_site_id.to_string(),
                "permissions": [permission],
            },
            "parents": [{"type": "Site", "id": principal_site_id.to_string()}]
        }));
    }

    let principal_parent = match typed_principal {
        Some(TypedPrincipal::Account { id, person_id }) => {
            entities.push(json!({
                "uid": {"type": "Account", "id": id.clone()},
                "attrs": {
                    "site_id": principal_site_id.to_string(),
                    "person_id": person_id,
                    "role_ids": role_ids,
                },
                "parents": [{"type": "Site", "id": principal_site_id.to_string()}]
            }));
            json!({"type": "Account", "id": id})
        }
        Some(TypedPrincipal::Assistant { id, key_id }) => {
            entities.push(json!({
                "uid": {"type": "Assistant", "id": id.clone()},
                "attrs": {
                    "site_id": principal_site_id.to_string(),
                    "key_id": key_id,
                    "permissions": permissions,
                },
                "parents": [{"type": "Site", "id": principal_site_id.to_string()}]
            }));
            json!({"type": "Assistant", "id": id})
        }
        None => json!({"type": "Site", "id": principal_site_id.to_string()}),
    };
    entities.push(json!({
        "uid": {"type": "Principal", "id": principal_id},
        "attrs": {
            "site_id": principal_site_id.to_string(),
            "grants": grants.as_slice().iter().map(|grant| format!("{}:{}", grant.capability.as_str(), grant.action.as_str())).collect::<Vec<_>>(),
            "permissions": permissions,
            "plugins": principal_plugins,
        },
        "parents": [principal_parent]
    }));
    entities.push(json!({
        "uid": {"type": "Resource", "id": resource_id},
        "attrs": {
            "site_id": resource_site_id_string,
            "kind": resource_type,
            "grants": resource_grants.as_slice().iter().map(|grant| format!("{}:{}", grant.capability.as_str(), grant.action.as_str())).collect::<Vec<_>>(),
            "permissions": resource_permissions,
            "plugins": [plugin.as_str()],
        },
        "parents": [{"type": "PluginResource", "id": plugin.as_str()}]
    }));

    Entities::from_json_value(Value::Array(entities), None).map_err(|_| MaviError::Internal)
}

fn legacy_business_aliases(grant: Grant) -> Vec<String> {
    let mut aliases =
        vec![Permission::from_legacy(grant.capability, grant.action).capability_key()];
    match (grant.capability, grant.action) {
        // The first role API exposed one broad People capability. Preserve
        // that compatibility contract while new clients use the narrower
        // business actions directly.
        (mavi_core::Capability::People, mavi_core::Action::View) => aliases.extend(
            [
                "core.people.view",
                "core.roles.list",
                "core.credentials.list",
            ]
            .map(str::to_owned),
        ),
        (mavi_core::Capability::People, mavi_core::Action::Write) => aliases.extend(
            [
                "core.people.create",
                "core.roles.manage",
                "core.credentials.manage",
            ]
            .map(str::to_owned),
        ),
        (mavi_core::Capability::People, mavi_core::Action::Delete) => {
            aliases.extend(["core.roles.manage", "core.credentials.revoke"].map(str::to_owned));
        }
        (mavi_core::Capability::Automation, mavi_core::Action::View) => {
            aliases.extend([
                Permission::new(PluginId::Automation, "workflow.view").capability_key(),
                Permission::new(PluginId::Automation, "flow.run.view").capability_key(),
            ]);
        }
        (mavi_core::Capability::Automation, mavi_core::Action::Write) => {
            aliases
                .push(Permission::new(PluginId::Automation, "workflow.control").capability_key());
        }
        _ => {}
    }
    aliases
}

fn permission_applies(held: &Permission, needed: &Permission, resource_type: &str) -> bool {
    held.plugin == needed.plugin
        && held.action == needed.action
        && held
            .resource_type
            .as_deref()
            .is_none_or(|expected| expected == resource_type)
}

fn uid(entity_type: &str, id: &str) -> Result<EntityUid, MaviError> {
    format!("{entity_type}::{id:?}")
        .parse()
        .map_err(|_| MaviError::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_core::{Action, Caller, Capability, Grant, Grants, Permission, PluginId, RequestId};

    fn request(grants: Grants, grant: Grant) -> AuthorizationRequest {
        let site_id = SiteId::new();
        AuthorizationRequest {
            principal_id: "person".to_owned(),
            principal_site_id: site_id,
            grants,
            resource_grants: Grants::default(),
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

    #[test]
    fn cedar_allows_a_matching_resource_grant_without_a_global_grant() {
        let authorizer = CedarAuthorizer::new().expect("policy");
        let needed = Grant::new(Capability::Content, Action::Write);
        let site_id = SiteId::new();
        let request = AuthorizationRequest {
            principal_id: "instructor".to_owned(),
            principal_site_id: site_id,
            grants: Grants::default(),
            resource_grants: Grants::new([needed]),
            grant: needed,
            resource_type: "Course".to_owned(),
            resource_id: "course".to_owned(),
            resource_site_id: site_id,
            request_site_id: site_id,
        };
        assert!(authorizer.authorize(&request).is_ok());

        let mut denied = request;
        denied.resource_grants = Grants::new([Grant::new(Capability::Content, Action::View)]);
        assert!(authorizer.authorize(&denied).is_err());
    }

    #[test]
    fn public_permissioned_requests_are_unauthenticated() {
        let authorizer = CedarAuthorizer::new().expect("policy");
        let context = SiteContext::public(SiteId::new());

        assert!(matches!(
            authorizer.authorize_context(
                &context,
                Grant::new(Capability::Content, Action::View),
                "Content",
                "collection",
                context.site_id,
            ),
            Err(MaviError::Unauthenticated)
        ));
    }

    fn account_context(site_id: SiteId, grants: Grants) -> SiteContext {
        SiteContext::with_caller(
            site_id,
            Caller::Account {
                person_id: mavi_core::PersonId::new(),
                session_id: None,
                grants,
            },
            RequestId::new(),
        )
    }

    #[test]
    fn business_permission_is_namespaced_and_plugin_aware() {
        let authorizer = CedarAuthorizer::new().expect("policy");
        let site_id = SiteId::new();
        let active = [PluginId::Core, PluginId::Writing].into_iter().collect();
        let context = account_context(
            site_id,
            Grants::new([Grant::new(Capability::Content, Action::Write)]),
        );
        let permission = Permission::new(PluginId::Writing, "content.entry.update");

        assert!(
            authorizer
                .authorize_permission(
                    &context,
                    &permission,
                    "Content",
                    "entry-1",
                    site_id,
                    &active,
                )
                .is_ok()
        );

        let typed_permission =
            Permission::new(PluginId::Writing, "content.entry.update").for_resource("Content");
        assert!(
            authorizer
                .authorize_permission(
                    &context,
                    &typed_permission,
                    "Content",
                    "entry-1",
                    site_id,
                    &active,
                )
                .is_ok()
        );
        assert!(
            authorizer
                .authorize_permission(
                    &context,
                    &typed_permission,
                    "Course",
                    "course-1",
                    site_id,
                    &active,
                )
                .is_err()
        );

        let disabled = [PluginId::Core].into_iter().collect();
        assert!(
            authorizer
                .authorize_permission(
                    &context,
                    &permission,
                    "Content",
                    "entry-1",
                    site_id,
                    &disabled,
                )
                .is_err()
        );
    }

    #[test]
    fn legacy_grants_are_translated_before_cedar_evaluation() {
        let authorizer = CedarAuthorizer::new().expect("policy");
        let site_id = SiteId::new();
        let active = [PluginId::Core, PluginId::Writing].into_iter().collect();
        let context = account_context(
            site_id,
            Grants::new([Grant::new(Capability::Content, Action::Write)]),
        );

        assert!(
            authorizer
                .authorize_legacy_grant(
                    &context,
                    Grant::new(Capability::Content, Action::Write),
                    "Content",
                    "entry-1",
                    site_id,
                    &active,
                    &Grants::default(),
                )
                .is_ok()
        );

        let disabled = [PluginId::Core].into_iter().collect();
        assert!(
            authorizer
                .authorize_legacy_grant(
                    &context,
                    Grant::new(Capability::Content, Action::Write),
                    "Content",
                    "entry-1",
                    site_id,
                    &disabled,
                    &Grants::default(),
                )
                .is_err()
        );
    }

    #[test]
    fn plugin_activation_is_owner_only_and_cross_site_is_denied() {
        let authorizer = CedarAuthorizer::new().expect("policy");
        let site_id = SiteId::new();
        let active = [PluginId::Core, PluginId::Writing].into_iter().collect();
        let owner = account_context(
            site_id,
            Grants::new([Grant::new(Capability::People, Action::Write)]),
        );
        let activation = Permission::new(PluginId::Core, "plugins.activate");
        assert!(
            authorizer
                .authorize_permission(&owner, &activation, "Plugin", "commerce", site_id, &active,)
                .is_ok()
        );

        let other_site = SiteId::new();
        assert!(
            authorizer
                .authorize_permission(
                    &owner,
                    &activation,
                    "Plugin",
                    "commerce",
                    other_site,
                    &active,
                )
                .is_err()
        );
    }
}
