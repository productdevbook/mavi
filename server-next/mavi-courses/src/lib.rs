//! Site-scoped learning: course authoring, student identity and progress.
//!
//! Panel accounts and students are deliberately different principals. Panel
//! routes require the `courses` Cedar grants; learning routes require a
//! course-owned student session and never consult panel grants.

mod auth;
mod courses;
mod learning;
mod relocation;
mod students;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_core::{Cursor, MaviError, Result};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use auth::{StudentActivationInput, StudentLoginInput, StudentSessionCreated};
pub use courses::{
    Course, CourseListFilter, CourseState, CourseSummary, CreateCourse, CreateLesson, CreateModule,
    Lesson, LessonListFilter, Module, ReorderLessons, ReorderModules, UpdateCourse, UpdateLesson,
    UpdateModule,
};
pub use learning::{LearningCourse, LearningCourseListFilter, LearningLesson, Progress};
pub use relocation::{
    CourseLessonRelocation, CourseModuleRelocation, CourseRelocation,
    CourseStudentCredentialRelocation, CourseStudentRelocation, CoursesRelocation,
    EnrollmentRelocation, ProgressRelocation,
};
pub use students::{
    CreateStudent, EnrollStudent, Enrollment, EnrollmentListFilter, Student, StudentInvitation,
    StudentListFilter, StudentStanding, UpdateStudent,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct CoursesService;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RecentCursor {
    pub created_at: DateTime<Utc>,
    pub id: Uuid,
}

pub(crate) fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes =
        serde_json::to_vec(&RecentCursor { created_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

pub(crate) fn decode_cursor(cursor: &Cursor) -> Result<RecentCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

pub(crate) async fn audit(
    tx: &mut SiteTx,
    context: &mavi_core::SiteContext,
    action: &str,
    resource_type: &str,
    resource_id: Option<Uuid>,
    payload: serde_json::Value,
) -> Result<()> {
    mavi_audit::AuditService
        .record(
            tx,
            context,
            &mavi_audit::AuditEntry {
                action: action.to_owned(),
                resource_type: resource_type.to_owned(),
                resource_id,
                payload,
            },
        )
        .await
}

#[must_use]
pub fn api() -> mavi_contract::Api {
    let mut api = courses::api();
    api.extend(students::api());
    api.extend(auth::api());
    api.extend(learning::api());
    api
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn course_cursor_is_opaque_and_bounded() {
        let created_at = Utc::now();
        let id = Uuid::now_v7();
        let cursor = encode_cursor(created_at, id).expect("cursor");
        let decoded = decode_cursor(&cursor).expect("decoded cursor");
        assert_eq!(decoded.created_at, created_at);
        assert_eq!(decoded.id, id);
        assert!(!cursor.as_str().contains("offset"));
        assert!(!cursor.as_str().contains("page"));
    }

    #[test]
    fn course_api_is_complete_and_cursor_only() {
        let api = api();
        api.validate().expect("course API");
        let contract = serde_json::to_string(&api).expect("contract");
        assert!(contract.contains("courses.list"));
        assert!(contract.contains("learning.lesson"));
        assert!(contract.contains("student"));
        assert!(!contract.contains("offset"));
        assert!(!contract.contains("page_number"));
    }
}
