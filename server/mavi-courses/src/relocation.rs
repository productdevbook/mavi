use std::collections::BTreeSet;
use std::fmt;

use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_core::{MaviError, Result, SiteContext, SiteId};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use super::{CourseState, CoursesService, StudentStanding};

pub const COURSES_RELOCATION_FORMAT: &str = "mavi.courses.relocation";
pub const COURSES_RELOCATION_VERSION: u16 = 1;
pub const MAX_COURSES_RELOCATION_RECORDS: usize = 100_000;
pub const MAX_COURSES_RELOCATION_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CoursesRelocation {
    pub format: String,
    pub version: u16,
    pub source_site_id: SiteId,
    pub courses: Vec<CourseRelocation>,
    pub modules: Vec<CourseModuleRelocation>,
    pub lessons: Vec<CourseLessonRelocation>,
    pub students: Vec<CourseStudentRelocation>,
    pub student_credentials: Vec<CourseStudentCredentialRelocation>,
    pub enrollments: Vec<EnrollmentRelocation>,
    pub progress: Vec<ProgressRelocation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CourseRelocation {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub about: Option<String>,
    pub state: CourseState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CourseModuleRelocation {
    pub id: Uuid,
    pub course_id: Uuid,
    pub title: String,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CourseLessonRelocation {
    pub id: Uuid,
    pub module_id: Uuid,
    pub title: String,
    pub body: String,
    pub media_file_id: Option<Uuid>,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CourseStudentRelocation {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub standing: StudentStanding,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Password hashes are private relocation material. Invitation hashes and
/// session tokens are intentionally absent: a target must issue fresh ones.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CourseStudentCredentialRelocation {
    pub student_id: Uuid,
    pub password_hash: String,
}

impl fmt::Debug for CourseStudentCredentialRelocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CourseStudentCredentialRelocation")
            .field("student_id", &self.student_id)
            .field("password_hash", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentRelocation {
    pub id: Uuid,
    pub course_id: Uuid,
    pub student_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressRelocation {
    pub student_id: Uuid,
    pub lesson_id: Uuid,
    pub completed_at: DateTime<Utc>,
}

impl CoursesRelocation {
    #[must_use]
    pub fn empty(source_site_id: SiteId) -> Self {
        Self {
            format: COURSES_RELOCATION_FORMAT.to_owned(),
            version: COURSES_RELOCATION_VERSION,
            source_site_id,
            courses: Vec::new(),
            modules: Vec::new(),
            lessons: Vec::new(),
            students: Vec::new(),
            student_credentials: Vec::new(),
            enrollments: Vec::new(),
            progress: Vec::new(),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn validate_for_relocation(&self, target_site: SiteId) -> Result<()> {
        if self.format != COURSES_RELOCATION_FORMAT {
            return Err(MaviError::validation("courses_relocation_format_invalid"));
        }
        if self.version != COURSES_RELOCATION_VERSION {
            return Err(MaviError::validation(
                "courses_relocation_version_unsupported",
            ));
        }
        if self.source_site_id != target_site || self.source_site_id.into_uuid().is_nil() {
            return Err(MaviError::conflict("courses_relocation_site_mismatch"));
        }
        let sections = [
            self.courses.len(),
            self.modules.len(),
            self.lessons.len(),
            self.students.len(),
            self.student_credentials.len(),
            self.enrollments.len(),
            self.progress.len(),
        ];
        let total = sections
            .iter()
            .try_fold(0usize, |total, count| total.checked_add(*count))
            .ok_or_else(|| MaviError::validation("courses_relocation_count_overflow"))?;
        if total > MAX_COURSES_RELOCATION_RECORDS
            || sections
                .iter()
                .any(|count| *count > MAX_COURSES_RELOCATION_RECORDS)
        {
            return Err(MaviError::validation("courses_relocation_counts_invalid"));
        }

        let mut course_ids = BTreeSet::new();
        for course in &self.courses {
            if course.id.is_nil()
                || !course_ids.insert(course.id)
                || !valid_slug(&course.slug)
                || !valid_text(&course.title, 300)
                || !course
                    .about
                    .as_deref()
                    .is_none_or(|value| valid_text(value, 10_000))
            {
                return Err(MaviError::validation("courses_relocation_course_invalid"));
            }
        }

        let mut module_ids = BTreeSet::new();
        let mut module_positions = BTreeSet::new();
        for module in &self.modules {
            if module.id.is_nil()
                || !module_ids.insert(module.id)
                || !course_ids.contains(&module.course_id)
                || !valid_text(&module.title, 300)
                || module.position < 0
                || !module_positions.insert((module.course_id, module.position))
            {
                return Err(MaviError::validation("courses_relocation_module_invalid"));
            }
        }

        let mut lesson_ids = BTreeSet::new();
        let mut lesson_positions = BTreeSet::new();
        for lesson in &self.lessons {
            if lesson.id.is_nil()
                || !lesson_ids.insert(lesson.id)
                || !module_ids.contains(&lesson.module_id)
                || !valid_text(&lesson.title, 300)
                || lesson.body.chars().count() > 100_000
                || lesson.body.contains('\0')
                || lesson.position < 0
                || !lesson_positions.insert((lesson.module_id, lesson.position))
            {
                return Err(MaviError::validation("courses_relocation_lesson_invalid"));
            }
        }

        let mut student_ids = BTreeSet::new();
        let mut emails = BTreeSet::new();
        for student in &self.students {
            if student.id.is_nil()
                || !student_ids.insert(student.id)
                || !valid_email(&student.email)
                || !emails.insert(student.email.clone())
                || !valid_text(&student.name, 200)
            {
                return Err(MaviError::validation("courses_relocation_student_invalid"));
            }
        }

        let mut credential_ids = BTreeSet::new();
        for credential in &self.student_credentials {
            if !student_ids.contains(&credential.student_id)
                || !credential_ids.insert(credential.student_id)
                || credential.password_hash.trim().is_empty()
                || credential.password_hash.len() > 1_024
            {
                return Err(MaviError::validation(
                    "courses_relocation_student_credential_invalid",
                ));
            }
        }
        for student in &self.students {
            let has_password = credential_ids.contains(&student.id);
            if (student.standing == StudentStanding::Asked) == has_password {
                return Err(MaviError::validation(
                    "courses_relocation_student_credential_state_invalid",
                ));
            }
        }

        let mut enrollment_ids = BTreeSet::new();
        let mut enrollment_pairs = BTreeSet::new();
        for enrollment in &self.enrollments {
            if enrollment.id.is_nil()
                || !enrollment_ids.insert(enrollment.id)
                || !course_ids.contains(&enrollment.course_id)
                || !student_ids.contains(&enrollment.student_id)
                || !enrollment_pairs.insert((enrollment.course_id, enrollment.student_id))
                || enrollment
                    .finished_at
                    .is_some_and(|finished| finished < enrollment.started_at)
            {
                return Err(MaviError::validation(
                    "courses_relocation_enrollment_invalid",
                ));
            }
        }

        let mut progress_pairs = BTreeSet::new();
        for progress in &self.progress {
            if !student_ids.contains(&progress.student_id)
                || !lesson_ids.contains(&progress.lesson_id)
                || !progress_pairs.insert((progress.student_id, progress.lesson_id))
            {
                return Err(MaviError::validation("courses_relocation_progress_invalid"));
            }
        }

        if serde_json::to_vec(self)
            .map_err(|_| MaviError::Internal)?
            .len()
            > MAX_COURSES_RELOCATION_BYTES
        {
            return Err(MaviError::validation("courses_relocation_too_large"));
        }
        Ok(())
    }

    pub fn record_count(&self) -> Result<i64> {
        let count = self
            .courses
            .len()
            .checked_add(self.modules.len())
            .and_then(|value| value.checked_add(self.lessons.len()))
            .and_then(|value| value.checked_add(self.students.len()))
            .and_then(|value| value.checked_add(self.student_credentials.len()))
            .and_then(|value| value.checked_add(self.enrollments.len()))
            .and_then(|value| value.checked_add(self.progress.len()))
            .ok_or_else(|| MaviError::validation("courses_relocation_count_overflow"))?;
        i64::try_from(count).map_err(|_| MaviError::validation("courses_relocation_count_overflow"))
    }
}

impl CoursesService {
    #[allow(clippy::too_many_lines)]
    pub async fn export_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
    ) -> Result<CoursesRelocation> {
        let site_id = context.site_id.into_uuid();
        let courses = sqlx::query(
            "select id, slug, title, about, state, created_at, updated_at, deleted_at
               from courses where site_id = $1 order by created_at asc, id asc",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(CourseRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
                title: row.try_get("title").map_err(|_| MaviError::Internal)?,
                about: row.try_get("about").map_err(|_| MaviError::Internal)?,
                state: parse_course_state(
                    &row.try_get::<String, _>("state")
                        .map_err(|_| MaviError::Internal)?,
                )?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let modules = sqlx::query(
            "select id, course_id, title, position, created_at, updated_at
               from course_modules where site_id = $1 order by course_id, position, id",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(CourseModuleRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                course_id: row.try_get("course_id").map_err(|_| MaviError::Internal)?,
                title: row.try_get("title").map_err(|_| MaviError::Internal)?,
                position: row.try_get("position").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let lessons = sqlx::query(
            "select id, module_id, title, body, media_file_id, position, created_at, updated_at
               from course_lessons where site_id = $1 order by module_id, position, id",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(CourseLessonRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                module_id: row.try_get("module_id").map_err(|_| MaviError::Internal)?,
                title: row.try_get("title").map_err(|_| MaviError::Internal)?,
                body: row.try_get("body").map_err(|_| MaviError::Internal)?,
                media_file_id: row
                    .try_get("media_file_id")
                    .map_err(|_| MaviError::Internal)?,
                position: row.try_get("position").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let students = sqlx::query(
            "select id, email, name, standing, created_at, updated_at, deleted_at
               from course_students where site_id = $1 order by created_at asc, id asc",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(CourseStudentRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                email: row.try_get("email").map_err(|_| MaviError::Internal)?,
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                standing: parse_student_standing(
                    &row.try_get::<String, _>("standing")
                        .map_err(|_| MaviError::Internal)?,
                )?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let student_credentials = sqlx::query(
            "select id, password_hash from course_students
               where site_id = $1 and password_hash is not null order by id",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(CourseStudentCredentialRelocation {
                student_id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                password_hash: row
                    .try_get("password_hash")
                    .map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let enrollments = sqlx::query(
            "select id, course_id, student_id, started_at, finished_at, created_at
               from course_enrollments where site_id = $1 order by created_at asc, id asc",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(EnrollmentRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                course_id: row.try_get("course_id").map_err(|_| MaviError::Internal)?,
                student_id: row.try_get("student_id").map_err(|_| MaviError::Internal)?,
                started_at: row.try_get("started_at").map_err(|_| MaviError::Internal)?,
                finished_at: row
                    .try_get("finished_at")
                    .map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let progress = sqlx::query(
            "select student_id, lesson_id, completed_at from course_progress
               where site_id = $1 order by student_id, lesson_id",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(ProgressRelocation {
                student_id: row.try_get("student_id").map_err(|_| MaviError::Internal)?,
                lesson_id: row.try_get("lesson_id").map_err(|_| MaviError::Internal)?,
                completed_at: row
                    .try_get("completed_at")
                    .map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let relocation = CoursesRelocation {
            format: COURSES_RELOCATION_FORMAT.to_owned(),
            version: COURSES_RELOCATION_VERSION,
            source_site_id: context.site_id,
            courses,
            modules,
            lessons,
            students,
            student_credentials,
            enrollments,
            progress,
        };
        relocation.validate_for_relocation(context.site_id)?;
        Ok(relocation)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn import_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        relocation: &CoursesRelocation,
    ) -> Result<()> {
        relocation.validate_for_relocation(context.site_id)?;
        let site_id = context.site_id.into_uuid();

        for table in [
            "course_student_sessions",
            "course_progress",
            "course_enrollments",
            "course_lessons",
            "course_modules",
            "course_students",
            "courses",
        ] {
            let statement = match table {
                "course_student_sessions" => {
                    "delete from course_student_sessions where site_id = $1"
                }
                "course_progress" => "delete from course_progress where site_id = $1",
                "course_enrollments" => "delete from course_enrollments where site_id = $1",
                "course_lessons" => "delete from course_lessons where site_id = $1",
                "course_modules" => "delete from course_modules where site_id = $1",
                "course_students" => "delete from course_students where site_id = $1",
                "courses" => "delete from courses where site_id = $1",
                _ => return Err(MaviError::Internal),
            };
            sqlx::query(statement)
                .bind(site_id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
        }

        let credentials = relocation
            .student_credentials
            .iter()
            .map(|credential| (credential.student_id, credential.password_hash.as_str()))
            .collect::<std::collections::HashMap<_, _>>();
        for course in &relocation.courses {
            sqlx::query(
                "insert into courses
                    (site_id, id, slug, title, about, state, created_at, updated_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(site_id)
            .bind(course.id)
            .bind(&course.slug)
            .bind(&course.title)
            .bind(&course.about)
            .bind(course.state.as_str())
            .bind(course.created_at)
            .bind(course.updated_at)
            .bind(course.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        for module in &relocation.modules {
            sqlx::query(
                "insert into course_modules
                    (site_id, id, course_id, title, position, created_at, updated_at)
                 values ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(site_id)
            .bind(module.id)
            .bind(module.course_id)
            .bind(&module.title)
            .bind(module.position)
            .bind(module.created_at)
            .bind(module.updated_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        for lesson in &relocation.lessons {
            sqlx::query(
                "insert into course_lessons
                    (site_id, id, module_id, title, body, media_file_id, position, created_at, updated_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(site_id)
            .bind(lesson.id)
            .bind(lesson.module_id)
            .bind(&lesson.title)
            .bind(&lesson.body)
            .bind(lesson.media_file_id)
            .bind(lesson.position)
            .bind(lesson.created_at)
            .bind(lesson.updated_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        for student in &relocation.students {
            sqlx::query(
                "insert into course_students
                    (site_id, id, email, name, password_hash, standing,
                     activation_token_hash, activation_expires_at, created_at, updated_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, null, null, $7, $8, $9)",
            )
            .bind(site_id)
            .bind(student.id)
            .bind(&student.email)
            .bind(&student.name)
            .bind(credentials.get(&student.id).copied())
            .bind(student.standing.as_str())
            .bind(student.created_at)
            .bind(student.updated_at)
            .bind(student.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        for enrollment in &relocation.enrollments {
            sqlx::query(
                "insert into course_enrollments
                    (site_id, id, course_id, student_id, started_at, finished_at, created_at)
                 values ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(site_id)
            .bind(enrollment.id)
            .bind(enrollment.course_id)
            .bind(enrollment.student_id)
            .bind(enrollment.started_at)
            .bind(enrollment.finished_at)
            .bind(enrollment.created_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        for progress in &relocation.progress {
            sqlx::query(
                "insert into course_progress (site_id, student_id, lesson_id, completed_at)
                 values ($1, $2, $3, $4)",
            )
            .bind(site_id)
            .bind(progress.student_id)
            .bind(progress.lesson_id)
            .bind(progress.completed_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "portable.courses.relocated".to_owned(),
                    resource_type: "CoursesSnapshot".to_owned(),
                    resource_id: None,
                    payload: serde_json::json!({
                        "courses": relocation.courses.len(),
                        "modules": relocation.modules.len(),
                        "lessons": relocation.lessons.len(),
                        "students": relocation.students.len(),
                        "student_credentials": relocation.student_credentials.len(),
                        "enrollments": relocation.enrollments.len(),
                        "progress": relocation.progress.len(),
                        "student_sessions": 0,
                        "invitation_tokens": 0,
                    }),
                },
            )
            .await
    }
}

fn parse_course_state(value: &str) -> Result<CourseState> {
    match value {
        "draft" => Ok(CourseState::Draft),
        "open" => Ok(CourseState::Open),
        "closed" => Ok(CourseState::Closed),
        _ => Err(MaviError::Internal),
    }
}

fn parse_student_standing(value: &str) -> Result<StudentStanding> {
    match value {
        "asked" => Ok(StudentStanding::Asked),
        "learning" => Ok(StudentStanding::Learning),
        "stopped" => Ok(StudentStanding::Stopped),
        _ => Err(MaviError::Internal),
    }
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.contains('\0')
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        })
}

fn valid_email(value: &str) -> bool {
    let mut parts = value.split('@');
    let Some(local) = parts.next() else {
        return false;
    };
    let Some(domain) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && !local.is_empty()
        && local.len() <= 64
        && !domain.is_empty()
        && domain.contains('.')
        && value.len() <= 254
        && value == value.to_ascii_lowercase()
        && !value.chars().any(char::is_whitespace)
}
