//! What somebody may do.
//!
//! A grant is a capability and an access — `content:write` — held by a role as
//! data rather than written into the code, so a site can name a role of its
//! own. What a handler asks for is declared beside it, and the router refuses
//! before the handler runs.
//!
//! `own` is the one qualifier: a grant over one's own is checked against who
//! wrote the row, which is why a handler that answers about somebody else's
//! work asks again with the author in hand.
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::LazyLock;

use cedar_policy::{
    Authorizer, Context, Decision, Entities, Entity, EntityId, EntityTypeName, EntityUid,
    PolicySet, Request, RestrictedExpression, Schema,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::{AppError, Result};

const POLICIES: &str = include_str!("policies.cedar");
const SCHEMA: &str = include_str!("schema.cedarschema");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Content,
    Media,
    Taxonomy,
    Forms,
    Mail,
    Flows,
    Courses,
    Shop,
    People,
    Settings,
    Publish,
    Design,
    Boards,
    Audit,
}

impl Capability {
    pub const ALL: [Capability; 14] = [
        Capability::Content,
        Capability::Media,
        Capability::Taxonomy,
        Capability::Forms,
        Capability::Mail,
        Capability::Flows,
        Capability::Courses,
        Capability::Shop,
        Capability::People,
        Capability::Settings,
        Capability::Publish,
        Capability::Design,
        Capability::Boards,
        Capability::Audit,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Content => "content",
            Capability::Media => "media",
            Capability::Taxonomy => "taxonomy",
            Capability::Forms => "forms",
            Capability::Mail => "mail",
            Capability::Flows => "flows",
            Capability::Courses => "courses",
            Capability::Shop => "shop",
            Capability::People => "people",
            Capability::Settings => "settings",
            Capability::Publish => "publish",
            Capability::Design => "design",
            Capability::Boards => "boards",
            Capability::Audit => "audit",
        }
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    View,
    Write,
    Delete,
}

impl Access {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Access::View => "view",
            Access::Write => "write",
            Access::Delete => "delete",
        }
    }

    #[must_use]
    pub fn changes(self) -> bool {
        !matches!(self, Access::View)
    }
}

impl std::fmt::Display for Access {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A capability and the access wanted on it: `content:write`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Needs {
    pub capability: Capability,
    pub access: Access,
}

impl Needs {
    #[must_use]
    pub const fn new(capability: Capability, access: Access) -> Self {
        Self { capability, access }
    }

    #[must_use]
    pub fn grant(self) -> String {
        format!("{}:{}", self.capability.as_str(), self.access.as_str())
    }

    /// The same reach, over what the person made rather than over everything.
    #[must_use]
    pub fn own_grant(self) -> String {
        format!("{}:own", self.grant())
    }
}

impl std::fmt::Display for Needs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.capability, self.access)
    }
}

#[derive(Clone, Debug)]
pub enum Principal {
    Operator {
        id: Uuid,
    },
    Agency {
        id: Uuid,
        agency: Uuid,
        grants: HashSet<String>,
    },
    Customer {
        id: Uuid,
        site: Uuid,
    },
    SiteUser {
        id: Uuid,
        site: Uuid,
        grants: HashSet<String>,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum Resource {
    Site { id: Uuid, frozen: bool },
    Charge { id: Uuid, site: Uuid },
    Platform,
}

/// Proof that the engine was asked and answered yes.
///
/// Only [`check`] makes one, and the query functions in a domain take it, so a
/// read that never asked does not compile.
#[derive(Clone, Copy, Debug)]
pub struct Permit {
    needs: Needs,
}

impl Permit {
    #[must_use]
    pub fn needs(self) -> Needs {
        self.needs
    }
}

static ENGINE: LazyLock<Engine> = LazyLock::new(Engine::load);

#[derive(Debug)]
struct Engine {
    policies: PolicySet,
    schema: Schema,
    authorizer: Authorizer,
}

impl Engine {
    fn load() -> Self {
        let (schema, _warnings) =
            Schema::from_cedarschema_str(SCHEMA).expect("the bundled Cedar schema parses");

        let policies = PolicySet::from_str(POLICIES).expect("the bundled Cedar policies parse");

        Self {
            policies,
            schema,
            authorizer: Authorizer::new(),
        }
    }
}

/// Asks the engine. Nothing else decides; a resource no policy covers is
/// refused, which is what makes a forgotten policy closed rather than open.
pub fn check(
    principal: &Principal,
    needs: Needs,
    resource: Resource,
    owner: Option<Uuid>,
) -> Result<Permit> {
    let engine = &*ENGINE;

    let principal_uid = principal_uid(principal);
    let resource_uid = resource_uid(resource);

    let context = Context::from_pairs([
        (
            "needed".to_owned(),
            RestrictedExpression::new_string(needs.grant()),
        ),
        (
            "needed_own".to_owned(),
            RestrictedExpression::new_string(needs.own_grant()),
        ),
        (
            "is_change".to_owned(),
            RestrictedExpression::new_bool(needs.access.changes()),
        ),
        (
            "owner".to_owned(),
            RestrictedExpression::new_string(owner.map(|id| id.to_string()).unwrap_or_default()),
        ),
    ])
    .map_err(|_| AppError::Forbidden)?;

    let request = Request::new(
        principal_uid,
        action_uid(needs.access),
        resource_uid,
        context,
        Some(&engine.schema),
    )
    .map_err(|_| AppError::Forbidden)?;

    let entities = entities(principal, resource)?;
    let answer = engine
        .authorizer
        .is_authorized(&request, &engine.policies, &entities);

    match answer.decision() {
        Decision::Allow => Ok(Permit { needs }),
        Decision::Deny => Err(AppError::Forbidden),
    }
}

fn uid(kind: &str, id: &str) -> EntityUid {
    EntityUid::from_type_name_and_id(
        EntityTypeName::from_str(kind).expect("a type name from this file"),
        EntityId::from_str(id).expect("an id is always a valid entity id"),
    )
}

fn principal_uid(principal: &Principal) -> EntityUid {
    match principal {
        Principal::Operator { id } => uid("Operator", &id.to_string()),
        Principal::Agency { id, .. } => uid("AgencyUser", &id.to_string()),
        Principal::Customer { id, .. } => uid("Customer", &id.to_string()),
        Principal::SiteUser { id, .. } => uid("SiteUser", &id.to_string()),
    }
}

fn resource_uid(resource: Resource) -> EntityUid {
    match resource {
        Resource::Site { id, .. } => uid("Site", &id.to_string()),
        Resource::Charge { id, .. } => uid("Charge", &id.to_string()),
        Resource::Platform => uid("Platform", "platform"),
    }
}

fn action_uid(access: Access) -> EntityUid {
    uid("Action", access.as_str())
}

fn site_entity(id: Uuid, frozen: bool) -> Result<Entity> {
    Entity::new(
        uid("Site", &id.to_string()),
        [("frozen".to_owned(), RestrictedExpression::new_bool(frozen))]
            .into_iter()
            .collect(),
        HashSet::new(),
    )
    .map_err(|_| AppError::Forbidden)
}

fn entities(principal: &Principal, resource: Resource) -> Result<Entities> {
    let mut all = Vec::new();

    match resource {
        Resource::Site { id, frozen } => all.push(site_entity(id, frozen)?),
        Resource::Charge { id, site } => {
            all.push(site_entity(site, false)?);
            all.push(
                Entity::new(
                    uid("Charge", &id.to_string()),
                    [(
                        "site".to_owned(),
                        RestrictedExpression::new_entity_uid(uid("Site", &site.to_string())),
                    )]
                    .into_iter()
                    .collect(),
                    HashSet::new(),
                )
                .map_err(|_| AppError::Forbidden)?,
            );
        }
        Resource::Platform => all.push(
            Entity::new(
                resource_uid(resource),
                [].into_iter().collect(),
                HashSet::new(),
            )
            .map_err(|_| AppError::Forbidden)?,
        ),
    }

    let principal_entity = match principal {
        Principal::Operator { .. } => Entity::new(
            principal_uid(principal),
            [].into_iter().collect(),
            HashSet::new(),
        ),
        Principal::Agency { agency, grants, .. } => Entity::new(
            principal_uid(principal),
            [("grants".to_owned(), grant_set(grants))]
                .into_iter()
                .collect(),
            [uid("Agency", &agency.to_string())].into_iter().collect(),
        ),
        Principal::Customer { site, .. } => Entity::new(
            principal_uid(principal),
            [(
                "site".to_owned(),
                RestrictedExpression::new_entity_uid(uid("Site", &site.to_string())),
            )]
            .into_iter()
            .collect(),
            HashSet::new(),
        ),
        // The site it belongs to, not the one it is reaching for: a principal
        // parented to the resource is a principal that is inside every site.
        Principal::SiteUser { id, site, grants } => {
            let own = uid("Site", &site.to_string());

            if !matches!(resource, Resource::Site { id, .. } if id == *site) {
                all.push(site_entity(*site, false)?);
            }

            Entity::new(
                principal_uid(principal),
                [
                    ("grants".to_owned(), grant_set(grants)),
                    (
                        "id".to_owned(),
                        RestrictedExpression::new_string(id.to_string()),
                    ),
                ]
                .into_iter()
                .collect(),
                [own].into_iter().collect(),
            )
        }
    }
    .map_err(|_| AppError::Forbidden)?;

    all.push(principal_entity);

    Entities::from_entities(all, None).map_err(|_| AppError::Forbidden)
}

fn grant_set(grants: &HashSet<String>) -> RestrictedExpression {
    RestrictedExpression::new_set(
        grants
            .iter()
            .map(|grant| RestrictedExpression::new_string(grant.clone())),
    )
}

/// Every grant there is, the whole-site ones and the own-only ones. The panel
/// draws its menu from what a role holds, and this is the list a role is
/// edited against.
#[must_use]
pub fn every_grant() -> Vec<String> {
    let mut all = Vec::with_capacity(Capability::ALL.len() * 6);

    for capability in Capability::ALL {
        for access in [Access::View, Access::Write, Access::Delete] {
            let needs = Needs::new(capability, access);
            all.push(needs.grant());
            all.push(needs.own_grant());
        }
    }

    all
}

#[cfg(test)]
mod tests;
