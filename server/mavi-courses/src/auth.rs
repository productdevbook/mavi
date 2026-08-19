use argon2::{Argon2, PasswordHasher, PasswordVerifier, password_hash::phc::PasswordHash};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use mavi_contract::{Endpoint, Method, Shape};
use mavi_core::{
    Caller, Email, ErrorCode, MaviError, Result, SiteContext, StudentId, StudentSessionId,
};
use mavi_storage::SiteTx;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::{CoursesService, Student, audit};

const SESSION_DAYS: i64 = 30;
const INVITATION_DAYS: i64 = 7;
const MAX_PASSWORD_CHARS: usize = 1024;

pub const STUDENT_CREDENTIALS_INVALID: &str = "course_student_credentials_invalid";
pub const STUDENT_INVITATION_INVALID: &str = "course_student_invitation_invalid";
pub const STUDENT_SESSION_NOT_FOUND: &str = "course_student_session_not_found";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StudentActivationInput {
    pub email: String,
    pub invitation_token: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StudentLoginInput {
    pub email: String,
    pub password: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct StudentSessionCreated {
    pub student: Student,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new([
        Endpoint::new(
            Method::Post,
            "/public/v1/courses/students/activate",
            "courses.students.activate",
            "Accept a student invitation and create a student session",
        )
        .public_mutation()
        .takes("StudentActivationInput")
        .returns(201, "StudentSessionCreated")
        .refuses([
            ErrorCode::Validation,
            ErrorCode::Unauthenticated,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/public/v1/courses/students/sessions",
            "courses.students.session.create",
            "Create a student learning session",
        )
        .public_mutation()
        .takes("StudentLoginInput")
        .returns(201, "StudentSessionCreated")
        .refuses([
            ErrorCode::Validation,
            ErrorCode::Unauthenticated,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/student/v1/auth/session",
            "courses.students.session.revoke",
            "Revoke the current student session",
        )
        .student_changes(false)
        .returns(204, "Empty")
        .refuses([
            ErrorCode::Unauthenticated,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
    ])
    .with_shapes([
        Shape::new(
            "StudentActivationInput",
            json!({
                "type": "object",
                "required": ["email", "invitation_token", "password"],
                "additionalProperties": false,
                "properties": {
                    "email": {"type": "string", "format": "email"},
                    "invitation_token": {"type": "string", "minLength": 16, "maxLength": 128},
                    "password": {"type": "string", "minLength": 12, "maxLength": MAX_PASSWORD_CHARS}
                }
            }),
        ),
        Shape::new(
            "StudentLoginInput",
            json!({
                "type": "object",
                "required": ["email", "password"],
                "additionalProperties": false,
                "properties": {
                    "email": {"type": "string", "format": "email"},
                    "password": {"type": "string", "minLength": 12, "maxLength": MAX_PASSWORD_CHARS}
                }
            }),
        ),
        Shape::new(
            "StudentSessionCreated",
            json!({
                "type": "object",
                "required": ["student", "token", "expires_at"],
                "properties": {
                    "student": {"$ref": "#/components/schemas/Student"},
                    "token": {"type": "string"},
                    "expires_at": {"type": "string", "format": "date-time"}
                }
            }),
        ),
    ])
}

impl CoursesService {
    pub async fn activate_student(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &StudentActivationInput,
        now: DateTime<Utc>,
    ) -> Result<StudentSessionCreated> {
        if !context.caller.is_public() {
            return Err(MaviError::Forbidden);
        }
        let email = Email::parse(&input.email)
            .map_err(|_| MaviError::validation_field(STUDENT_CREDENTIALS_INVALID, "email"))?;
        validate_password(&input.password)?;
        if input.invitation_token.trim().is_empty() {
            return Err(MaviError::validation_field(
                STUDENT_INVITATION_INVALID,
                "invitation_token",
            ));
        }
        let row = sqlx::query(
            "select id, email, name, standing, created_at, updated_at
               from course_students
              where site_id = $1 and email = $2 and activation_token_hash = $3
                and standing = 'asked' and deleted_at is null
                and activation_expires_at > $4
              for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(email.as_str())
        .bind(hash_token(input.invitation_token.trim()))
        .bind(now)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or_else(|| MaviError::conflict(STUDENT_INVITATION_INVALID))?;
        let student_id = StudentId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
        let digest = hash_password(&input.password)?;
        let updated = sqlx::query(
            "update course_students
                set password_hash = $3, standing = 'learning', activation_token_hash = null,
                    activation_expires_at = null, updated_at = $4
              where site_id = $1 and id = $2
              returning id, email, name, standing, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(student_id.into_uuid())
        .bind(digest)
        .bind(now)
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let student = student_from_row(&updated)?;
        audit(
            tx,
            context,
            "courses.student.activated",
            "CourseStudent",
            Some(student_id.into_uuid()),
            json!({}),
        )
        .await?;
        create_session(tx, context.site_id, student, now).await
    }

    pub async fn login_student(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &StudentLoginInput,
        now: DateTime<Utc>,
    ) -> Result<StudentSessionCreated> {
        if !context.caller.is_public() {
            return Err(MaviError::Forbidden);
        }
        let email = Email::parse(&input.email)
            .map_err(|_| MaviError::validation_field(STUDENT_CREDENTIALS_INVALID, "email"))?;
        validate_password(&input.password)?;
        let row = sqlx::query(
            "select id, email, name, standing, password_hash, created_at, updated_at
               from course_students
              where site_id = $1 and email = $2 and standing = 'learning' and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(email.as_str())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::Unauthenticated)?;
        let digest: String = row
            .try_get("password_hash")
            .map_err(|_| MaviError::Internal)?;
        if !verify_password(&input.password, &digest) {
            return Err(MaviError::Unauthenticated);
        }
        let student = student_from_row(&row)?;
        create_session(tx, context.site_id, student, now).await
    }

    pub async fn authenticate_student(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        token: &str,
        now: DateTime<Utc>,
    ) -> Result<Caller> {
        if token.trim().is_empty() {
            return Err(MaviError::Unauthenticated);
        }
        let row = sqlx::query(
            "select ss.id, ss.student_id
               from course_student_sessions ss
               join course_students s on s.site_id = ss.site_id and s.id = ss.student_id
              where ss.site_id = $1 and ss.token_hash = $2 and ss.expires_at > $3
                and ss.revoked_at is null and s.standing = 'learning' and s.deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(hash_token(token))
        .bind(now)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::Unauthenticated)?;
        Ok(Caller::Student {
            student_id: StudentId::from_uuid(
                row.try_get("student_id").map_err(|_| MaviError::Internal)?,
            ),
            session_id: Some(StudentSessionId::from_uuid(
                row.try_get("id").map_err(|_| MaviError::Internal)?,
            )),
        })
    }

    pub async fn logout_student(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let Caller::Student {
            student_id,
            session_id: Some(session_id),
        } = &context.caller
        else {
            return Err(MaviError::Unauthenticated);
        };
        let changed = sqlx::query(
            "update course_student_sessions set revoked_at = $4
               where site_id = $1 and id = $2 and student_id = $3 and revoked_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(session_id.into_uuid())
        .bind(student_id.into_uuid())
        .bind(now)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if changed.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: STUDENT_SESSION_NOT_FOUND,
            });
        }
        audit(
            tx,
            context,
            "courses.student.session.revoked",
            "CourseStudentSession",
            Some(session_id.into_uuid()),
            json!({"student_id": student_id}),
        )
        .await
    }
}

pub(crate) fn new_invitation() -> (String, Vec<u8>) {
    let token = new_token();
    let hash = hash_token(&token);
    (token, hash)
}

pub(crate) fn invitation_expires_at(now: DateTime<Utc>) -> DateTime<Utc> {
    now + Duration::days(INVITATION_DAYS)
}

async fn create_session(
    tx: &mut SiteTx,
    site_id: mavi_core::SiteId,
    student: Student,
    now: DateTime<Utc>,
) -> Result<StudentSessionCreated> {
    let session_id = StudentSessionId::new();
    let token = new_token();
    let expires_at = now + Duration::days(SESSION_DAYS);
    sqlx::query(
        "insert into course_student_sessions
            (site_id, id, student_id, token_hash, expires_at)
         values ($1, $2, $3, $4, $5)",
    )
    .bind(site_id.into_uuid())
    .bind(session_id.into_uuid())
    .bind(student.id.into_uuid())
    .bind(hash_token(&token))
    .bind(expires_at)
    .execute(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    Ok(StudentSessionCreated {
        student,
        token,
        expires_at,
    })
}

fn validate_password(value: &str) -> Result<()> {
    if !(12..=MAX_PASSWORD_CHARS).contains(&value.chars().count())
        || value.chars().any(char::is_control)
    {
        return Err(MaviError::validation(STUDENT_CREDENTIALS_INVALID));
    }
    Ok(())
}

fn hash_password(value: &str) -> Result<String> {
    Argon2::default()
        .hash_password(value.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|_| MaviError::Internal)
}

fn verify_password(value: &str, digest: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(digest) else {
        return false;
    };
    Argon2::default()
        .verify_password(value.as_bytes(), &parsed)
        .is_ok()
}

fn new_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

pub(crate) fn student_from_row(row: &sqlx::postgres::PgRow) -> Result<Student> {
    Ok(Student {
        id: StudentId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        email: row.try_get("email").map_err(|_| MaviError::Internal)?,
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        standing: crate::StudentStanding::parse(
            &row.try_get::<String, _>("standing")
                .map_err(|_| MaviError::Internal)?,
        )?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}
