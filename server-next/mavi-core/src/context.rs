use serde::{Deserialize, Serialize};

use crate::{
    ApiKeyId, Grants, PersonId, RequestId, SessionId, SiteId, StudentId, StudentSessionId,
};

/// The authenticated principal making a request to a site.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Caller {
    Public,
    Account {
        person_id: PersonId,
        session_id: Option<SessionId>,
        grants: Grants,
    },
    Student {
        student_id: StudentId,
        session_id: Option<StudentSessionId>,
    },
    Assistant {
        key_id: ApiKeyId,
        person_id: Option<PersonId>,
        grants: Grants,
    },
    /// A trusted in-process worker. This caller is never admitted by the
    /// HTTP authentication boundary; it exists so background mutations have
    /// an explicit audit principal instead of being recorded as public.
    System {
        worker: String,
    },
}

impl Caller {
    #[must_use]
    pub const fn is_public(&self) -> bool {
        matches!(self, Self::Public)
    }

    #[must_use]
    pub fn grants(&self) -> Option<&Grants> {
        match self {
            Self::Account { grants, .. } | Self::Assistant { grants, .. } => Some(grants),
            Self::Public | Self::Student { .. } | Self::System { .. } => None,
        }
    }
}

/// The only scope a site-owned application operation may run under.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SiteContext {
    pub site_id: SiteId,
    pub caller: Caller,
    pub request_id: RequestId,
}

impl SiteContext {
    #[must_use]
    pub fn public(site_id: SiteId) -> Self {
        Self {
            site_id,
            caller: Caller::Public,
            request_id: RequestId::new(),
        }
    }

    #[must_use]
    pub fn with_caller(site_id: SiteId, caller: Caller, request_id: RequestId) -> Self {
        Self {
            site_id,
            caller,
            request_id,
        }
    }

    #[must_use]
    pub fn system(site_id: SiteId, worker: impl Into<String>, request_id: RequestId) -> Self {
        Self::with_caller(
            site_id,
            Caller::System {
                worker: worker.into(),
            },
            request_id,
        )
    }
}
