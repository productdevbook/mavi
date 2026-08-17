use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Audit,
    Analytics,
    Automation,
    Boards,
    Content,
    Courses,
    Design,
    Forms,
    Mail,
    Media,
    People,
    Publish,
    Settings,
    Shop,
    Taxonomy,
    Trash,
}

impl Capability {
    pub const ALL: [Self; 16] = [
        Self::Audit,
        Self::Analytics,
        Self::Automation,
        Self::Boards,
        Self::Content,
        Self::Courses,
        Self::Design,
        Self::Forms,
        Self::Mail,
        Self::Media,
        Self::People,
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
            Self::Design => "design",
            Self::Forms => "forms",
            Self::Mail => "mail",
            Self::Media => "media",
            Self::People => "people",
            Self::Publish => "publish",
            Self::Settings => "settings",
            Self::Shop => "shop",
            Self::Taxonomy => "taxonomy",
            Self::Trash => "trash",
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
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Grants(Vec<Grant>);

impl Grants {
    #[must_use]
    pub fn new(grants: impl IntoIterator<Item = Grant>) -> Self {
        Self(grants.into_iter().collect())
    }

    #[must_use]
    pub fn allows(&self, needed: Grant) -> bool {
        self.0.contains(&needed)
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Grant] {
        &self.0
    }
}
