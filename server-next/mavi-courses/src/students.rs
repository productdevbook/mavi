use chrono::{DateTime, Utc};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, CourseId, Email, EnrollmentId, ErrorCode, MaviError, Page, PageRequest,
    Result, SiteContext, StudentId,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::auth::{invitation_expires_at, new_invitation, student_from_row};
use crate::courses::{COURSE_NOT_FOUND, CourseState};
use crate::{CoursesService, audit, decode_cursor, encode_cursor};

pub const STUDENT_NOT_FOUND: &str = "course_student_not_found";
pub const STUDENT_EMAIL_INVALID: &str = "course_student_email_invalid";
pub const STUDENT_NAME_INVALID: &str = "course_student_name_invalid";
pub const STUDENT_STANDING_INVALID: &str = "course_student_standing_invalid";
pub const STUDENT_NOT_LEARNING: &str = "course_student_not_learning";
pub const COURSE_NOT_OPEN: &str = "course_not_open";
pub const ENROLLMENT_NOT_FOUND: &str = "course_enrollment_not_found";
pub const STUDENT_EMAIL_TAKEN: &str = "course_student_email_taken";

const MAX_NAME_CHARS: usize = 200;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StudentStanding {
    Asked,
    Learning,
    Stopped,
}

impl StudentStanding {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Asked => "asked",
            Self::Learning => "learning",
            Self::Stopped => "stopped",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "asked" => Ok(Self::Asked),
            "learning" => Ok(Self::Learning),
            "stopped" => Ok(Self::Stopped),
            _ => Err(MaviError::Internal),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Student {
    pub id: StudentId,
    pub email: String,
    pub name: String,
    pub standing: StudentStanding,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateStudent {
    pub email: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateStudent {
    pub name: Option<String>,
    pub standing: Option<StudentStanding>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct StudentListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub standing: Option<StudentStanding>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StudentInvitation {
    pub student: Student,
    /// Returned once so the panel can hand the invitation to its mail layer.
    /// The database stores only the hash.
    pub invitation_token: String,
    pub invitation_expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollStudent {
    pub student_id: StudentId,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnrollmentListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Serialize)]
pub struct Enrollment {
    pub id: EnrollmentId,
    pub course_id: CourseId,
    pub student_id: StudentId,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn api() -> mavi_contract::Api {
    let view = Permission {
        capability: Capability::Courses,
        action: Action::View,
    };
    let write = Permission {
        capability: Capability::Courses,
        action: Action::Write,
    };
    let delete = Permission {
        capability: Capability::Courses,
        action: Action::Delete,
    };
    mavi_contract::Api::new([
        Endpoint::new(
            Method::Get,
            "/api/v1/courses/students",
            "courses.students.list",
            "List course students with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("StudentListFilter")
        .returns(200, "StudentPage")
        .refuses([ErrorCode::Forbidden, ErrorCode::Validation, ErrorCode::Internal]),
        Endpoint::new(
            Method::Post,
            "/api/v1/courses/students",
            "courses.students.create",
            "Create a student invitation",
        )
        .account_or_assistant()
        .requires(write)
        .takes("CreateStudent")
        .returns(201, "StudentInvitation")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/courses/students/{id}/invite",
            "courses.students.invite",
            "Rotate an unanswered student invitation",
        )
        .account_or_assistant()
        .requires(write)
        .returns(200, "StudentInvitation")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/courses/students/{id}",
            "courses.students.update",
            "Update a student name or standing",
        )
        .account_or_assistant()
        .requires(write)
        .takes("UpdateStudent")
        .returns(200, "Student")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/courses/{course_id}/enrollments",
            "courses.enrollments.list",
            "List enrollments for a course with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .resource_scoped()
        .takes_query("EnrollmentListFilter")
        .returns(200, "EnrollmentPage")
        .refuses([ErrorCode::Forbidden, ErrorCode::NotFound, ErrorCode::Validation, ErrorCode::Internal]),
        Endpoint::new(
            Method::Post,
            "/api/v1/courses/{course_id}/enrollments",
            "courses.enrollments.create",
            "Enroll a learning student idempotently",
        )
        .account_or_assistant()
        .requires(write)
        .resource_scoped()
        .takes("EnrollStudent")
        .returns(201, "Enrollment")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/courses/enrollments/{id}",
            "courses.enrollments.delete",
            "Remove an enrollment while retaining lesson progress",
        )
        .account_or_assistant()
        .requires(delete)
        .resource_scoped()
        .returns(204, "Empty")
        .changes(false)
        .refuses([ErrorCode::Forbidden, ErrorCode::NotFound, ErrorCode::Internal]),
    ])
    .with_shapes([
        Shape::new(
            "StudentStanding",
            json!({"type": "string", "enum": ["asked", "learning", "stopped"]}),
        ),
        Shape::new(
            "Student",
            json!({"type": "object", "required": ["id", "email", "name", "standing", "created_at", "updated_at"], "properties": {
                "id": {"type": "string", "format": "uuid"}, "email": {"type": "string", "format": "email"},
                "name": {"type": "string"}, "standing": {"$ref": "#/components/schemas/StudentStanding"},
                "created_at": {"type": "string", "format": "date-time"}, "updated_at": {"type": "string", "format": "date-time"}
            }}),
        ),
        Shape::new(
            "StudentListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512}, "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                "standing": {"$ref": "#/components/schemas/StudentStanding"}
            }}),
        ),
        Shape::new(
            "CreateStudent",
            json!({"type": "object", "required": ["email", "name"], "additionalProperties": false, "properties": {
                "email": {"type": "string", "format": "email"}, "name": {"type": "string", "minLength": 1, "maxLength": MAX_NAME_CHARS}
            }}),
        ),
        Shape::new(
            "UpdateStudent",
            json!({"type": "object", "additionalProperties": false, "properties": {
                "name": {"type": ["string", "null"], "maxLength": MAX_NAME_CHARS},
                "standing": {"oneOf": [{"$ref": "#/components/schemas/StudentStanding"}, {"type": "null"}]}
            }}),
        ),
        Shape::new(
            "StudentInvitation",
            json!({"type": "object", "required": ["student", "invitation_token", "invitation_expires_at"], "properties": {
                "student": {"$ref": "#/components/schemas/Student"}, "invitation_token": {"type": "string"},
                "invitation_expires_at": {"type": "string", "format": "date-time"}
            }}),
        ),
        Shape::new(
            "EnrollStudent",
            json!({"type": "object", "required": ["student_id"], "additionalProperties": false, "properties": {
                "student_id": {"type": "string", "format": "uuid"}
            }}),
        ),
        Shape::new(
            "EnrollmentListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512}, "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }}),
        ),
        Shape::new(
            "Enrollment",
            json!({"type": "object", "required": ["id", "course_id", "student_id", "started_at", "finished_at", "created_at"], "properties": {
                "id": {"type": "string", "format": "uuid"}, "course_id": {"type": "string", "format": "uuid"},
                "student_id": {"type": "string", "format": "uuid"}, "started_at": {"type": "string", "format": "date-time"},
                "finished_at": {"type": ["string", "null"], "format": "date-time"}, "created_at": {"type": "string", "format": "date-time"}
            }}),
        ),
        Shape::new(
            "StudentPage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/Student"}}, "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
        Shape::new(
            "EnrollmentPage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/Enrollment"}}, "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
    ])
}

impl CoursesService {
    pub async fn list_students(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &StudentListFilter,
    ) -> Result<Page<Student>> {
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, email, name, standing, created_at, updated_at
               from course_students where site_id = ",
        );
        query
            .push_bind(context.site_id.into_uuid())
            .push(" and deleted_at is null");
        if let Some(standing) = filter.standing {
            query.push(" and standing = ").push_bind(standing.as_str());
        }
        if let Some(after) = after {
            query
                .push(" and (created_at, id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        let rows = query
            .push(" order by created_at desc, id desc limit ")
            .push_bind(limit + 1)
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows
            .iter()
            .map(student_from_row)
            .collect::<Result<Vec<_>>>()?;
        let limit_usize = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit_usize {
            let last = items.get(limit_usize - 1).ok_or(MaviError::Internal)?;
            Some(encode_cursor(last.created_at, last.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit_usize);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn create_student(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateStudent,
        now: DateTime<Utc>,
    ) -> Result<StudentInvitation> {
        let email = Email::parse(&input.email)
            .map_err(|_| MaviError::validation_field(STUDENT_EMAIL_INVALID, "email"))?;
        let name = validate_name(&input.name)?;
        let id = StudentId::new();
        let (token, token_hash) = new_invitation();
        let expires_at = invitation_expires_at(now);
        let row = sqlx::query(
            "insert into course_students
                (site_id, id, email, name, activation_token_hash, activation_expires_at)
             values ($1, $2, $3, $4, $5, $6)
             returning id, email, name, standing, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(email.as_str())
        .bind(name)
        .bind(token_hash)
        .bind(expires_at)
        .fetch_one(tx.conn())
        .await
        .map_err(map_student_write_error)?;
        let student = student_from_row(&row)?;
        audit(
            tx,
            context,
            "courses.student.invited",
            "CourseStudent",
            Some(id.into_uuid()),
            json!({"email": student.email}),
        )
        .await?;
        Ok(StudentInvitation {
            student,
            invitation_token: token,
            invitation_expires_at: expires_at,
        })
    }

    pub async fn reissue_invitation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: StudentId,
        now: DateTime<Utc>,
    ) -> Result<StudentInvitation> {
        let row = sqlx::query(
            "select id, email, name, standing, created_at, updated_at
               from course_students
              where site_id = $1 and id = $2 and deleted_at is null
              for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: STUDENT_NOT_FOUND,
        })?;
        let standing = StudentStanding::parse(
            &row.try_get::<String, _>("standing")
                .map_err(|_| MaviError::Internal)?,
        )?;
        if standing != StudentStanding::Asked {
            return Err(MaviError::conflict(STUDENT_STANDING_INVALID));
        }
        let (token, token_hash) = new_invitation();
        let expires_at = invitation_expires_at(now);
        sqlx::query(
            "update course_students set activation_token_hash = $3,
                    activation_expires_at = $4, updated_at = $5
              where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(token_hash)
        .bind(expires_at)
        .bind(now)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let student = student_from_row(&row)?;
        audit(
            tx,
            context,
            "courses.student.invitation.reissued",
            "CourseStudent",
            Some(id.into_uuid()),
            json!({}),
        )
        .await?;
        Ok(StudentInvitation {
            student,
            invitation_token: token,
            invitation_expires_at: expires_at,
        })
    }

    pub async fn update_student(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: StudentId,
        input: &UpdateStudent,
    ) -> Result<Student> {
        let row = sqlx::query(
            "select id, email, name, standing, password_hash, created_at, updated_at
               from course_students where site_id = $1 and id = $2 and deleted_at is null
              for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: STUDENT_NOT_FOUND,
        })?;
        let current = StudentStanding::parse(
            &row.try_get::<String, _>("standing")
                .map_err(|_| MaviError::Internal)?,
        )?;
        let name = input.name.as_deref().map(validate_name).transpose()?;
        let next = input.standing.unwrap_or(current);
        if next == StudentStanding::Asked && current != StudentStanding::Asked {
            return Err(MaviError::conflict(STUDENT_STANDING_INVALID));
        }
        if current == StudentStanding::Asked && next == StudentStanding::Stopped {
            return Err(MaviError::conflict(STUDENT_STANDING_INVALID));
        }
        if next == StudentStanding::Learning
            && row
                .try_get::<Option<String>, _>("password_hash")
                .map_err(|_| MaviError::Internal)?
                .is_none()
        {
            return Err(MaviError::conflict(STUDENT_STANDING_INVALID));
        }
        if name.is_none() && input.standing.is_none() {
            return student_from_row(&row);
        }
        let updated = sqlx::query(
            "update course_students set name = coalesce($3, name), standing = $4,
                    updated_at = clock_timestamp()
              where site_id = $1 and id = $2
              returning id, email, name, standing, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(name)
        .bind(next.as_str())
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let student = student_from_row(&updated)?;
        audit(
            tx,
            context,
            "courses.student.updated",
            "CourseStudent",
            Some(id.into_uuid()),
            json!({"standing": student.standing}),
        )
        .await?;
        Ok(student)
    }

    pub async fn list_enrollments(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        course_id: CourseId,
        filter: &EnrollmentListFilter,
    ) -> Result<Page<Enrollment>> {
        ensure_course_exists(tx, context, course_id).await?;
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, course_id, student_id, started_at, finished_at, created_at
               from course_enrollments where site_id = ",
        );
        query
            .push_bind(context.site_id.into_uuid())
            .push(" and course_id = ")
            .push_bind(course_id.into_uuid());
        if let Some(after) = after {
            query
                .push(" and (created_at, id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        let rows = query
            .push(" order by created_at desc, id desc limit ")
            .push_bind(limit + 1)
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows
            .iter()
            .map(enrollment_from_row)
            .collect::<Result<Vec<_>>>()?;
        let limit_usize = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit_usize {
            let last = items.get(limit_usize - 1).ok_or(MaviError::Internal)?;
            Some(encode_cursor(last.created_at, last.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit_usize);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn enroll(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        course_id: CourseId,
        input: &EnrollStudent,
    ) -> Result<Enrollment> {
        ensure_course_open(tx, context, course_id).await?;
        let standing: String = sqlx::query_scalar(
            "select standing from course_students
              where site_id = $1 and id = $2 and deleted_at is null for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(input.student_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: STUDENT_NOT_FOUND,
        })?;
        if StudentStanding::parse(&standing)? != StudentStanding::Learning {
            return Err(MaviError::conflict(STUDENT_NOT_LEARNING));
        }
        let id = EnrollmentId::new();
        let row = sqlx::query(
            "insert into course_enrollments (site_id, id, course_id, student_id)
             values ($1, $2, $3, $4)
             on conflict (site_id, student_id, course_id) do nothing
             returning id, course_id, student_id, started_at, finished_at, created_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(course_id.into_uuid())
        .bind(input.student_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let (enrollment, created) = if let Some(row) = row {
            (enrollment_from_row(&row)?, true)
        } else {
            let row = sqlx::query(
                "select id, course_id, student_id, started_at, finished_at, created_at
                   from course_enrollments
                  where site_id = $1 and course_id = $2 and student_id = $3",
            )
            .bind(context.site_id.into_uuid())
            .bind(course_id.into_uuid())
            .bind(input.student_id.into_uuid())
            .fetch_one(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            (enrollment_from_row(&row)?, false)
        };
        if created {
            audit(
                tx,
                context,
                "courses.enrollment.created",
                "CourseEnrollment",
                Some(enrollment.id.into_uuid()),
                json!({"course_id": course_id, "student_id": input.student_id}),
            )
            .await?;
        }
        Ok(enrollment)
    }

    pub async fn unenroll(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: EnrollmentId,
    ) -> Result<()> {
        let row = sqlx::query(
            "delete from course_enrollments
              where site_id = $1 and id = $2
              returning course_id, student_id",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: ENROLLMENT_NOT_FOUND,
        })?;
        audit(
            tx,
            context,
            "courses.enrollment.deleted",
            "CourseEnrollment",
            Some(id.into_uuid()),
            json!({
                "course_id": row.try_get::<Uuid, _>("course_id").map_err(|_| MaviError::Internal)?,
                "student_id": row.try_get::<Uuid, _>("student_id").map_err(|_| MaviError::Internal)?
            }),
        )
        .await
    }
}

async fn ensure_course_exists(
    tx: &mut SiteTx,
    context: &SiteContext,
    course_id: CourseId,
) -> Result<()> {
    let exists: bool = sqlx::query_scalar(
        "select exists(select 1 from courses where site_id = $1 and id = $2 and deleted_at is null)",
    )
    .bind(context.site_id.into_uuid())
    .bind(course_id.into_uuid())
    .fetch_one(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    if exists {
        Ok(())
    } else {
        Err(MaviError::NotFound {
            resource: COURSE_NOT_FOUND,
        })
    }
}

async fn ensure_course_open(
    tx: &mut SiteTx,
    context: &SiteContext,
    course_id: CourseId,
) -> Result<()> {
    let state: Option<String> = sqlx::query_scalar(
        "select state from courses where site_id = $1 and id = $2 and deleted_at is null for update",
    )
    .bind(context.site_id.into_uuid())
    .bind(course_id.into_uuid())
    .fetch_optional(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    let state = state.ok_or(MaviError::NotFound {
        resource: COURSE_NOT_FOUND,
    })?;
    if CourseState::parse(&state)? == CourseState::Open {
        Ok(())
    } else {
        Err(MaviError::conflict(COURSE_NOT_OPEN))
    }
}

fn validate_name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_NAME_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(MaviError::validation(STUDENT_NAME_INVALID));
    }
    Ok(value.to_owned())
}

fn map_student_write_error(error: sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error
        && database.constraint() == Some("course_students_site_email_active")
    {
        return MaviError::conflict(STUDENT_EMAIL_TAKEN);
    }
    MaviError::Internal
}

fn enrollment_from_row(row: &sqlx::postgres::PgRow) -> Result<Enrollment> {
    Ok(Enrollment {
        id: EnrollmentId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        course_id: CourseId::from_uuid(row.try_get("course_id").map_err(|_| MaviError::Internal)?),
        student_id: StudentId::from_uuid(
            row.try_get("student_id").map_err(|_| MaviError::Internal)?,
        ),
        started_at: row.try_get("started_at").map_err(|_| MaviError::Internal)?,
        finished_at: row
            .try_get("finished_at")
            .map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_and_enrollment_contracts_are_explicit() {
        assert_eq!(StudentStanding::Asked.as_str(), "asked");
        let contract = serde_json::to_string(&api()).expect("contract");
        assert!(contract.contains("StudentInvitation"));
        assert!(contract.contains("EnrollmentPage"));
        assert!(!contract.contains("offset"));
    }
}
