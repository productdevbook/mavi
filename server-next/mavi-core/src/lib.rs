//! Shared primitives for the clean Mavi implementation.
//!
//! This crate deliberately knows nothing about HTTP, `PostgreSQL` or a domain.
//! A type belongs here only when every domain needs the same meaning.

mod context;
mod email;
mod error;
mod grants;
mod ids;
mod money;
mod pagination;
pub mod ports;

pub use context::{Caller, SiteContext};
pub use email::Email;
pub use error::{ErrorCode, MaviError, Result};
pub use grants::{Action, Capability, Grant, Grants};
pub use ids::{
    AnalyticsEventId, ApiKeyId, AuditEventId, BoardCardId, BoardCommentId, BoardId, BoardListId,
    ContentId, CouponId, CouponUseId, CourseId, CredentialId, DesignBuildId, DesignChangeId,
    EnrollmentId, FileId, FlowId, FlowRunId, FlowRunStepId, FlowStepId, FormId, FormSubmissionId,
    JobId, LessonId, MailAttemptId, MailDeliveryId, MailListId, MailReaderId, MailTemplateId,
    ModuleId, OrderId, OrderLineId, PersonId, ProductId, RequestId, RoleId, SessionId, SiteId,
    StockHoldId, StudentId, StudentSessionId, TermId,
};
pub use money::{Currency, Money};
pub use pagination::{Cursor, Page, PageRequest};
