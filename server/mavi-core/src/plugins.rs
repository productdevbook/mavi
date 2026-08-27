use std::{fmt, str::FromStr};

use crate::{Action, Capability, Grant};
use serde::{Deserialize, Serialize};

/// A product capability compiled into the Mavi binary.
///
/// Plugins are deliberately closed over a small, typed set. Enabling a
/// plugin changes which compiled capabilities are exposed for a site; it does
/// not load native code from the database or from an untrusted directory.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PluginId {
    #[default]
    Core,
    Writing,
    Commerce,
    Learning,
    Forms,
    Messaging,
    Automation,
    Boards,
    Analytics,
    Governance,
}

impl PluginId {
    pub const ALL: [Self; 10] = [
        Self::Core,
        Self::Writing,
        Self::Commerce,
        Self::Learning,
        Self::Forms,
        Self::Messaging,
        Self::Automation,
        Self::Boards,
        Self::Analytics,
        Self::Governance,
    ];

    pub const DEFAULT_ENABLED: [Self; 2] = [Self::Core, Self::Writing];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Writing => "writing",
            Self::Commerce => "commerce",
            Self::Learning => "learning",
            Self::Forms => "forms",
            Self::Messaging => "messaging",
            Self::Automation => "automation",
            Self::Boards => "boards",
            Self::Analytics => "analytics",
            Self::Governance => "governance",
        }
    }

    #[must_use]
    pub const fn is_core(self) -> bool {
        matches!(self, Self::Core)
    }

    #[must_use]
    pub const fn dependencies(self) -> &'static [Self] {
        match self {
            Self::Core => &[],
            Self::Writing
            | Self::Commerce
            | Self::Learning
            | Self::Forms
            | Self::Messaging
            | Self::Automation
            | Self::Boards
            | Self::Analytics
            | Self::Governance => &[Self::Core],
        }
    }

    /// The business actions compiled into each plugin.
    ///
    /// This is the single permission catalog shared by the application
    /// registry, Cedar schema validation and the role-management contract.
    /// Transport verbs deliberately do not appear here.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub const fn business_actions(self) -> &'static [&'static str] {
        match self {
            Self::Core => &[
                "plugins.list",
                "plugins.activate",
                "plugins.deactivate",
                "people.list",
                "people.view",
                "people.create",
                "people.update",
                "people.delete",
                "roles.list",
                "roles.manage",
                "sessions.revoke",
                "credentials.list",
                "credentials.manage",
                "credentials.revoke",
                "settings.view",
                "settings.manage",
                "settings.delete",
                "feedback.view",
                "feedback.submit",
                "feedback.delete",
            ],
            Self::Writing => &[
                "content.entry.list",
                "content.entry.create",
                "content.entry.update",
                "content.entry.delete",
                "content.entry.publish",
                "content.type.manage",
                "taxonomy.term.manage",
                "taxonomy.term.view",
                "taxonomy.term.delete",
                "media.file.list",
                "media.file.manage",
                "media.file.delete",
                "design.change.view",
                "design.change.manage",
                "design.change.delete",
                "design.build",
                "design.publish",
                "site.view",
                "site.publish",
            ],
            Self::Commerce => &[
                "shop.product.view",
                "shop.product.manage",
                "shop.product.delete",
                "shop.order.view",
                "shop.order.manage",
                "shop.order.fulfill",
                "shop.coupon.manage",
            ],
            Self::Learning => &[
                "courses.course.view",
                "courses.course.manage",
                "courses.course.delete",
                "courses.module.manage",
                "courses.lesson.view",
                "courses.lesson.manage",
                "courses.student.view",
                "courses.student.manage",
                "courses.enrollment.manage",
                "courses.progress.view",
            ],
            Self::Forms => &[
                "form.manage",
                "form.delete",
                "submission.view",
                "submission.manage",
            ],
            Self::Messaging => &[
                "template.manage",
                "list.manage",
                "delivery.view",
                "delivery.manage",
                "delivery.delete",
            ],
            Self::Automation => &[
                "flow.view",
                "flow.manage",
                "flow.delete",
                "flow.start",
                "flow.run.view",
                "workflow.view",
                "workflow.control",
            ],
            Self::Boards => &["board.view", "board.manage", "board.delete", "card.manage"],
            Self::Analytics => &["view", "manage"],
            Self::Governance => &[
                "audit.view",
                "audit.manage",
                "trash.manage",
                "trash.view",
                "trash.delete",
                "portable.export",
                "portable.import",
                "portable.delete",
            ],
        }
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PluginId {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "core" => Ok(Self::Core),
            "writing" => Ok(Self::Writing),
            "commerce" => Ok(Self::Commerce),
            "learning" => Ok(Self::Learning),
            "forms" => Ok(Self::Forms),
            "messaging" => Ok(Self::Messaging),
            "automation" => Ok(Self::Automation),
            "boards" => Ok(Self::Boards),
            "analytics" => Ok(Self::Analytics),
            "governance" => Ok(Self::Governance),
            _ => Err(()),
        }
    }
}

/// A business authorization action. HTTP verbs and transport details do not
/// belong in the permission model.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ActionId(String);

impl ActionId {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        let value = self.as_str();
        !value.is_empty()
            && value.len() <= 160
            && value.split('.').all(|segment| {
                !segment.is_empty()
                    && segment.chars().enumerate().all(|(index, character)| {
                        character.is_ascii_alphanumeric()
                            || (index > 0 && matches!(character, '_' | '-'))
                    })
            })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Namespaced business permission used by Cedar and application use-cases.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Permission {
    pub plugin: PluginId,
    pub action: ActionId,
    pub resource_type: Option<String>,
}

impl Permission {
    /// Converts the compatibility capability/action pair used by the first
    /// API surface into the stable business-action vocabulary. New code
    /// should construct a permission with [`Self::new`] directly; keeping the
    /// conversion here makes old role records and endpoint declarations
    /// converge on one Cedar-facing representation.
    #[must_use]
    pub fn from_legacy(capability: Capability, action: Action) -> Self {
        let action = match (capability, action) {
            (Capability::Audit, Action::View) => "audit.view",
            (Capability::Audit, Action::Write | Action::Delete) => "audit.manage",
            (Capability::Analytics, Action::View) => "view",
            (Capability::Analytics, Action::Write | Action::Delete) => "manage",
            (Capability::Automation, Action::View) => "flow.view",
            (Capability::Automation, Action::Write) => "flow.manage",
            (Capability::Automation, Action::Delete) => "flow.delete",
            (Capability::Boards, Action::View) => "board.view",
            (Capability::Boards, Action::Write) => "board.manage",
            (Capability::Boards, Action::Delete) => "board.delete",
            (Capability::Content | Capability::Publish, Action::View) => "content.entry.list",
            (Capability::Content, Action::Write) => "content.entry.update",
            (Capability::Content | Capability::Publish, Action::Delete) => "content.entry.delete",
            (Capability::Courses, Action::View) => "courses.course.view",
            (Capability::Courses, Action::Write) => "courses.course.manage",
            (Capability::Courses, Action::Delete) => "courses.course.delete",
            (Capability::Credentials, Action::View) => "credentials.list",
            (Capability::Credentials, Action::Write) => "credentials.manage",
            (Capability::Credentials, Action::Delete) => "credentials.revoke",
            (Capability::Design, Action::View) => "design.change.view",
            (Capability::Design, Action::Write) => "design.change.manage",
            (Capability::Design, Action::Delete) => "design.change.delete",
            (Capability::Feedback, Action::View) => "feedback.view",
            (Capability::Feedback, Action::Write) => "feedback.submit",
            (Capability::Feedback, Action::Delete) => "feedback.delete",
            (Capability::Forms, Action::View) => "submission.view",
            (Capability::Forms, Action::Write) => "form.manage",
            (Capability::Forms, Action::Delete) => "form.delete",
            (Capability::Mail, Action::View) => "delivery.view",
            (Capability::Mail, Action::Write) => "delivery.manage",
            (Capability::Mail, Action::Delete) => "delivery.delete",
            (Capability::Media, Action::View) => "media.file.list",
            (Capability::Media, Action::Write) => "media.file.manage",
            (Capability::Media, Action::Delete) => "media.file.delete",
            (Capability::People, Action::View) => "people.list",
            (Capability::People, Action::Write) => "people.update",
            (Capability::People, Action::Delete) => "people.delete",
            (Capability::Portable, Action::View) => "portable.export",
            (Capability::Portable, Action::Write) => "portable.import",
            (Capability::Portable, Action::Delete) => "portable.delete",
            (Capability::Publish, Action::Write) => "content.entry.publish",
            (Capability::Settings, Action::View) => "settings.view",
            (Capability::Settings, Action::Write) => "settings.manage",
            (Capability::Settings, Action::Delete) => "settings.delete",
            (Capability::Shop, Action::View) => "shop.product.view",
            (Capability::Shop, Action::Write) => "shop.product.manage",
            (Capability::Shop, Action::Delete) => "shop.product.delete",
            (Capability::Taxonomy, Action::View) => "taxonomy.term.view",
            (Capability::Taxonomy, Action::Write) => "taxonomy.term.manage",
            (Capability::Taxonomy, Action::Delete) => "taxonomy.term.delete",
            (Capability::Trash, Action::View) => "trash.view",
            (Capability::Trash, Action::Write) => "trash.manage",
            (Capability::Trash, Action::Delete) => "trash.delete",
        };
        Self::new(capability.plugin(), action)
    }

    #[must_use]
    pub fn new(plugin: PluginId, action: impl Into<String>) -> Self {
        Self {
            plugin,
            action: ActionId::new(action),
            resource_type: None,
        }
    }

    /// Parses the canonical value persisted by the identity store. The
    /// resource type is deliberately not encoded in this key; it is stored
    /// in its own nullable column so `None` and a resource-scoped permission
    /// cannot become ambiguous strings.
    #[must_use]
    pub fn from_key(value: &str) -> Option<Self> {
        let (plugin, action) = value.split_once('.')?;
        let plugin = plugin.parse().ok()?;
        let permission = Self::new(plugin, action);
        (permission.is_valid()
            && plugin
                .business_actions()
                .contains(&permission.action.as_str()))
        .then_some(permission)
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.action.is_valid()
            && self
                .resource_type
                .as_deref()
                .is_none_or(|resource| !resource.trim().is_empty() && resource.len() <= 120)
    }

    #[must_use]
    pub fn for_resource(mut self, resource_type: impl Into<String>) -> Self {
        self.resource_type = Some(resource_type.into());
        self
    }

    /// Returns the broad capability/action representation used by older
    /// domain ports. The mapping is intentionally one-way: a legacy grant is
    /// less precise than a business permission, so the canonical permission
    /// remains the authorization source of truth.
    #[must_use]
    pub fn to_legacy(&self) -> Option<Grant> {
        let action = self.action.as_str();
        let grant = match self.plugin {
            PluginId::Core => {
                let capability = if action.starts_with("plugins.")
                    || action.starts_with("people.")
                    || action.starts_with("roles.")
                {
                    Capability::People
                } else if action.starts_with("credentials.") {
                    Capability::Credentials
                } else if action.starts_with("settings.") {
                    Capability::Settings
                } else if action.starts_with("feedback.") {
                    Capability::Feedback
                } else {
                    return None;
                };
                Grant::new(capability, legacy_action(action))
            }
            PluginId::Writing => {
                let capability = if action.starts_with("content.") {
                    if action == "content.entry.publish" {
                        return Some(Grant::new(Capability::Publish, Action::Write));
                    }
                    Capability::Content
                } else if action.starts_with("taxonomy.") {
                    Capability::Taxonomy
                } else if action.starts_with("media.") {
                    Capability::Media
                } else if action.starts_with("design.") {
                    Capability::Design
                } else {
                    return None;
                };
                Grant::new(capability, legacy_action(action))
            }
            PluginId::Commerce => Grant::new(Capability::Shop, legacy_action(action)),
            PluginId::Learning => Grant::new(Capability::Courses, legacy_action(action)),
            PluginId::Forms => Grant::new(Capability::Forms, legacy_action(action)),
            PluginId::Messaging => Grant::new(Capability::Mail, legacy_action(action)),
            PluginId::Automation => Grant::new(Capability::Automation, legacy_action(action)),
            PluginId::Boards => Grant::new(Capability::Boards, legacy_action(action)),
            PluginId::Analytics => Grant::new(Capability::Analytics, legacy_action(action)),
            PluginId::Governance => {
                let capability = if action.starts_with("audit.") {
                    Capability::Audit
                } else if action.starts_with("trash.") {
                    Capability::Trash
                } else if action.starts_with("portable.") {
                    Capability::Portable
                } else {
                    return None;
                };
                Grant::new(capability, legacy_action(action))
            }
        };
        Some(grant)
    }

    #[must_use]
    pub fn capability_key(&self) -> String {
        format!("{}.{}", self.plugin, self.action)
    }
}

fn legacy_action(action: &str) -> Action {
    match action.rsplit('.').next().unwrap_or(action) {
        "list" | "view" | "export" => Action::View,
        "delete" | "revoke" => Action::Delete,
        _ => Action::Write,
    }
}
