use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Audit,
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    View,
    Write,
    Delete,
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
