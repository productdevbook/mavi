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

/// Who is asking. One kind of person: somebody with an account on this site,
/// carrying whatever their role grants.
///
/// There were four, and three of them — an operator over many installations,
/// an agency renting sites, a customer reading a bill — were a hosting
/// business's people rather than a site's. Not one was ever constructed
/// outside a test.
#[derive(Clone, Debug)]
pub struct Principal {
    pub id: Uuid,
    pub grants: HashSet<String>,
}

/// The one thing anything is ever asked about. There is no `Resource` type
/// because there is nothing to choose between: an installation is a site, and
/// a question about "which site" is the question this whole change removes.
const THE_SITE: &str = "site";

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
pub fn check(principal: &Principal, needs: Needs, owner: Option<Uuid>) -> Result<Permit> {
    let engine = &*ENGINE;

    let principal_uid = principal_uid(principal);
    let resource_uid = uid("Site", THE_SITE);

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

    let entities = entities(principal)?;
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
    uid("SiteUser", &principal.id.to_string())
}

fn action_uid(access: Access) -> EntityUid {
    uid("Action", access.as_str())
}

fn entities(principal: &Principal) -> Result<Entities> {
    let site = Entity::new(
        uid("Site", THE_SITE),
        [].into_iter().collect(),
        HashSet::new(),
    )
    .map_err(|_| AppError::Forbidden)?;

    let asking = Entity::new(
        principal_uid(principal),
        [
            ("grants".to_owned(), grant_set(&principal.grants)),
            (
                "id".to_owned(),
                RestrictedExpression::new_string(principal.id.to_string()),
            ),
        ]
        .into_iter()
        .collect(),
        HashSet::new(),
    )
    .map_err(|_| AppError::Forbidden)?;

    Entities::from_entities([site, asking], None).map_err(|_| AppError::Forbidden)
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
