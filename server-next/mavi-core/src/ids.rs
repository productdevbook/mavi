use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn into_uuid(self) -> Uuid {
                self.0
            }

            #[must_use]
            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self::from_uuid(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.into_uuid()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

typed_id!(SiteId);
typed_id!(AuditEventId);
typed_id!(DesignChangeId);
typed_id!(DesignBuildId);
typed_id!(FormId);
typed_id!(FormSubmissionId);
typed_id!(MailTemplateId);
typed_id!(MailListId);
typed_id!(MailReaderId);
typed_id!(MailDeliveryId);
typed_id!(MailAttemptId);
typed_id!(ProductId);
typed_id!(CouponId);
typed_id!(OrderId);
typed_id!(OrderLineId);
typed_id!(StockHoldId);
typed_id!(CouponUseId);
typed_id!(CourseId);
typed_id!(ModuleId);
typed_id!(LessonId);
typed_id!(EnrollmentId);
typed_id!(StudentSessionId);
typed_id!(FlowId);
typed_id!(FlowStepId);
typed_id!(FlowRunId);
typed_id!(FlowRunStepId);
typed_id!(BoardId);
typed_id!(BoardListId);
typed_id!(BoardCardId);
typed_id!(BoardCommentId);
typed_id!(AnalyticsEventId);
typed_id!(JobId);
typed_id!(ContentId);
typed_id!(FileId);
typed_id!(ApiKeyId);
typed_id!(CredentialId);
typed_id!(PersonId);
typed_id!(RoleId);
typed_id!(RequestId);
typed_id!(SessionId);
typed_id!(StudentId);
typed_id!(TermId);
