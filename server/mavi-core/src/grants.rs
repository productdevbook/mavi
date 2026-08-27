use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Permission, PluginId};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Audit,
    Analytics,
    Automation,
    Boards,
    Content,
    Courses,
    Credentials,
    Design,
    Feedback,
    Forms,
    Mail,
    Media,
    People,
    Portable,
    Publish,
    Settings,
    Shop,
    Taxonomy,
    Trash,
}

impl Capability {
    pub const ALL: [Self; 19] = [
        Self::Audit,
        Self::Analytics,
        Self::Automation,
        Self::Boards,
        Self::Content,
        Self::Courses,
        Self::Credentials,
        Self::Design,
        Self::Feedback,
        Self::Forms,
        Self::Mail,
        Self::Media,
        Self::People,
        Self::Portable,
        Self::Publish,
        Self::Settings,
        Self::Shop,
        Self::Taxonomy,
        Self::Trash,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Audit => "audit",
            Self::Analytics => "analytics",
            Self::Automation => "automation",
            Self::Boards => "boards",
            Self::Content => "content",
            Self::Courses => "courses",
            Self::Credentials => "credentials",
            Self::Design => "design",
            Self::Feedback => "feedback",
            Self::Forms => "forms",
            Self::Mail => "mail",
            Self::Media => "media",
            Self::People => "people",
            Self::Portable => "portable",
            Self::Publish => "publish",
            Self::Settings => "settings",
            Self::Shop => "shop",
            Self::Taxonomy => "taxonomy",
            Self::Trash => "trash",
        }
    }

    #[must_use]
    pub const fn plugin(self) -> PluginId {
        match self {
            Self::Audit | Self::Portable | Self::Trash => PluginId::Governance,
            Self::Analytics => PluginId::Analytics,
            Self::Automation => PluginId::Automation,
            Self::Boards => PluginId::Boards,
            Self::Content | Self::Design | Self::Media | Self::Publish | Self::Taxonomy => {
                PluginId::Writing
            }
            Self::Courses => PluginId::Learning,
            Self::Forms => PluginId::Forms,
            Self::Mail => PluginId::Messaging,
            Self::Shop => PluginId::Commerce,
            Self::Feedback | Self::Credentials | Self::People | Self::Settings => PluginId::Core,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    View,
    Write,
    Delete,
}

impl Action {
    pub const ALL: [Self; 3] = [Self::View, Self::Write, Self::Delete];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Write => "write",
            Self::Delete => "delete",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Grant {
    pub capability: Capability,
    pub action: Action,
}

impl Grant {
    #[must_use]
    pub const fn new(capability: Capability, action: Action) -> Self {
        Self { capability, action }
    }

    #[must_use]
    pub fn permission_key(self) -> String {
        format!(
            "{}.{}.{}",
            self.capability.plugin(),
            self.capability.as_str(),
            self.action.as_str()
        )
    }
}

/// A compatibility grant collection that also carries the canonical
/// Cedar-facing permissions loaded from storage. Domain crates can continue
/// using the narrow `Grant` port while application authorization evaluates the
/// namespaced values without losing resource scope.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Grants {
    grants: Vec<Grant>,
    permissions: Vec<Permission>,
}

impl Serialize for Grants {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.grants.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Grants {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Vec::<Grant>::deserialize(deserializer).map(Self::new)
    }
}

impl Grants {
    #[must_use]
    pub fn new(grants: impl IntoIterator<Item = Grant>) -> Self {
        let grants = unique_grants(grants);
        let permissions = grants
            .iter()
            .map(|grant| Permission::from_legacy(grant.capability, grant.action))
            .collect();
        Self {
            grants,
            permissions,
        }
    }

    /// Builds a collection from canonical permissions while retaining the
    /// broad compatibility projection needed by older domain ports.
    #[must_use]
    pub fn from_permissions(permissions: impl IntoIterator<Item = Permission>) -> Self {
        let permissions = unique_permissions(permissions);
        let grants = unique_grants(permissions.iter().filter_map(Permission::to_legacy));
        Self {
            grants,
            permissions,
        }
    }

    /// Keeps the legacy pair for old domain code and the exact stored values
    /// for Cedar. This is used while forward-only migrations still expose the
    /// old columns to compatibility readers.
    #[must_use]
    pub fn with_permissions(
        grants: impl IntoIterator<Item = Grant>,
        permissions: impl IntoIterator<Item = Permission>,
    ) -> Self {
        let grants = unique_grants(grants);
        let mut permissions = unique_permissions(permissions);
        for grant in &grants {
            let compatibility = Permission::from_legacy(grant.capability, grant.action);
            if !permissions.contains(&compatibility) {
                permissions.push(compatibility);
            }
        }
        Self {
            grants,
            permissions,
        }
    }

    #[must_use]
    pub fn allows(&self, needed: Grant) -> bool {
        self.grants.contains(&needed)
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Grant] {
        &self.grants
    }

    #[must_use]
    pub fn permissions(&self) -> &[Permission] {
        &self.permissions
    }

    /// Checks the canonical permission first and falls back to the legacy
    /// capability projection for callers that have not yet migrated their
    /// role-management payload. A resource-scoped requirement never falls
    /// back to an unscoped grant.
    #[must_use]
    pub fn allows_permission(&self, needed: &Permission) -> bool {
        self.permissions.iter().any(|held| {
            held.plugin == needed.plugin
                && held.action == needed.action
                && (held.resource_type.is_none() || held.resource_type == needed.resource_type)
        }) || (needed.resource_type.is_none()
            && needed.to_legacy().is_some_and(|legacy| self.allows(legacy)))
    }
}

fn unique_grants(grants: impl IntoIterator<Item = Grant>) -> Vec<Grant> {
    let mut unique = Vec::new();
    for grant in grants {
        if !unique.contains(&grant) {
            unique.push(grant);
        }
    }
    unique
}

fn unique_permissions(permissions: impl IntoIterator<Item = Permission>) -> Vec<Permission> {
    let mut unique = Vec::new();
    for permission in permissions {
        if !unique.contains(&permission) {
            unique.push(permission);
        }
    }
    unique
}
