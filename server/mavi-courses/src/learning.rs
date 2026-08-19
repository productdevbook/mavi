use chrono::{DateTime, Utc};
use mavi_contract::{Endpoint, Method, Shape};
use mavi_core::{
    CourseId, ErrorCode, LessonId, MaviError, Page, PageRequest, Result, SiteContext, StudentId,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::courses::{CourseState, LESSON_NOT_FOUND, Lesson, lesson_from_row};
use crate::students::StudentStanding;
use crate::{CoursesService, audit, decode_cursor, encode_cursor};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct LearningCourseListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Serialize)]
pub struct LearningCourse {
    pub course_id: CourseId,
    pub slug: String,
    pub title: String,
    pub about: Option<String>,
    pub state: CourseState,
    pub completed_lessons: i64,
    pub total_lessons: i64,
    pub enrolled_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LearningLesson {
    pub lesson: Lesson,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub lesson_id: LessonId,
    pub completed_at: DateTime<Utc>,
}

#[must_use]
pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new([
        Endpoint::new(
            Method::Get,
            "/student/v1/learning/courses",
            "learning.courses.list",
            "List the courses for the current student with an opaque cursor",
        )
        .student()
        .takes_query("LearningCourseListFilter")
        .returns(200, "LearningCoursePage")
        .refuses([
            ErrorCode::Unauthenticated,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/student/v1/learning/lessons/{id}",
            "learning.lesson.read",
            "Read a lesson only when the student is enrolled and the course is open",
        )
        .student()
        .returns(200, "LearningLesson")
        .refuses([
            ErrorCode::Unauthenticated,
            ErrorCode::NotFound,
            ErrorCode::Forbidden,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/student/v1/learning/lessons/{id}/media",
            "learning.lesson.media.read",
            "Read the lesson media only when the student can access the lesson",
        )
        .student()
        .returns_raw(200, "FileBytes")
        .refuses([
            ErrorCode::Unauthenticated,
            ErrorCode::NotFound,
            ErrorCode::Forbidden,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Put,
            "/student/v1/learning/lessons/{id}/done",
            "learning.lesson.done",
            "Mark an accessible lesson complete idempotently",
        )
        .student_changes(true)
        .returns(200, "Progress")
        .refuses([
            ErrorCode::Unauthenticated,
            ErrorCode::NotFound,
            ErrorCode::Forbidden,
            ErrorCode::Internal,
        ]),
    ])
    .with_shapes([
        Shape::new(
            "LearningCourseListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }}),
        ),
        Shape::new(
            "LearningCourse",
            json!({"type": "object", "required": ["course_id", "slug", "title", "about", "state", "completed_lessons", "total_lessons", "enrolled_at"], "properties": {
                "course_id": {"type": "string", "format": "uuid"}, "slug": {"type": "string"}, "title": {"type": "string"},
                "about": {"type": ["string", "null"]}, "state": {"$ref": "#/components/schemas/CourseState"},
                "completed_lessons": {"type": "integer", "format": "int64", "minimum": 0}, "total_lessons": {"type": "integer", "format": "int64", "minimum": 0},
                "enrolled_at": {"type": "string", "format": "date-time"}
            }}),
        ),
        Shape::new(
            "LearningCoursePage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/LearningCourse"}}, "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
        Shape::new(
            "LearningLesson",
            json!({"type": "object", "required": ["lesson", "completed_at"], "properties": {
                "lesson": {"$ref": "#/components/schemas/Lesson"}, "completed_at": {"type": ["string", "null"], "format": "date-time"}
            }}),
        ),
        Shape::new(
            "Progress",
            json!({"type": "object", "required": ["lesson_id", "completed_at"], "properties": {
                "lesson_id": {"type": "string", "format": "uuid"}, "completed_at": {"type": "string", "format": "date-time"}
            }}),
        ),
        Shape::new("FileBytes", json!({"type": "string", "format": "binary"})),
    ])
}

impl CoursesService {
    pub async fn list_learning_courses(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &LearningCourseListFilter,
    ) -> Result<Page<LearningCourse>> {
        let student_id = current_student(context)?;
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select c.id, c.slug, c.title, c.about, c.state, e.created_at as enrolled_at,
                    (select count(*) from course_lessons l
                       join course_modules m on m.site_id = l.site_id and m.id = l.module_id
                      where l.site_id = c.site_id and m.course_id = c.id) as total_lessons,
                    (select count(*) from course_progress p
                       join course_lessons l on l.site_id = p.site_id and l.id = p.lesson_id
                       join course_modules m on m.site_id = l.site_id and m.id = l.module_id
                      where p.site_id = c.site_id and p.student_id = e.student_id and m.course_id = c.id) as completed_lessons
               from course_enrollments e
               join courses c on c.site_id = e.site_id and c.id = e.course_id
              where e.site_id = ",
        );
        query
            .push_bind(context.site_id.into_uuid())
            .push(" and e.student_id = ")
            .push_bind(student_id.into_uuid())
            .push(" and c.deleted_at is null");
        if let Some(after) = after {
            query
                .push(" and (e.created_at, e.id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        let rows = query
            .push(" order by e.created_at desc, e.id desc limit ")
            .push_bind(limit + 1)
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows
            .iter()
            .map(learning_course_from_row)
            .collect::<Result<Vec<_>>>()?;
        let limit_usize = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit_usize {
            let row = rows.get(limit_usize - 1).ok_or(MaviError::Internal)?;
            Some(encode_cursor(
                row.try_get("enrolled_at")
                    .map_err(|_| MaviError::Internal)?,
                row.try_get::<Uuid, _>("id")
                    .map_err(|_| MaviError::Internal)?,
            )?)
        } else {
            None
        };
        items.truncate(limit_usize);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn get_learning_lesson(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        lesson_id: LessonId,
    ) -> Result<LearningLesson> {
        let student_id = current_student(context)?;
        let row = sqlx::query(
            "select l.id, l.module_id, l.title, l.body, l.media_file_id, l.position,
                    l.created_at, l.updated_at, c.state, s.standing,
                    exists (select 1 from course_enrollments e
                             where e.site_id = c.site_id and e.course_id = c.id
                               and e.student_id = $3) as enrolled,
                    (select p.completed_at from course_progress p
                      where p.site_id = l.site_id and p.student_id = $3 and p.lesson_id = l.id) as completed_at
               from course_lessons l
               join course_modules m on m.site_id = l.site_id and m.id = l.module_id
               join courses c on c.site_id = m.site_id and c.id = m.course_id
               join course_students s on s.site_id = c.site_id and s.id = $3
              where l.site_id = $1 and l.id = $2 and c.deleted_at is null and s.deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(lesson_id.into_uuid())
        .bind(student_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: LESSON_NOT_FOUND,
        })?;
        let standing = StudentStanding::parse(
            &row.try_get::<String, _>("standing")
                .map_err(|_| MaviError::Internal)?,
        )?;
        if standing == StudentStanding::Stopped {
            return Err(MaviError::Forbidden);
        }
        let enrolled: bool = row.try_get("enrolled").map_err(|_| MaviError::Internal)?;
        if !enrolled {
            return Err(MaviError::Forbidden);
        }
        let state = CourseState::parse(
            &row.try_get::<String, _>("state")
                .map_err(|_| MaviError::Internal)?,
        )?;
        if !state.is_open() {
            return Err(MaviError::Forbidden);
        }
        Ok(LearningLesson {
            lesson: lesson_from_row(&row)?,
            completed_at: row
                .try_get("completed_at")
                .map_err(|_| MaviError::Internal)?,
        })
    }

    pub async fn complete_lesson(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        lesson_id: LessonId,
        now: DateTime<Utc>,
    ) -> Result<Progress> {
        let student_id = current_student(context)?;
        self.get_learning_lesson(tx, context, lesson_id).await?;
        let inserted: Option<DateTime<Utc>> = sqlx::query_scalar(
            "insert into course_progress (site_id, student_id, lesson_id, completed_at)
             values ($1, $2, $3, $4)
             on conflict (site_id, student_id, lesson_id) do nothing
             returning completed_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(student_id.into_uuid())
        .bind(lesson_id.into_uuid())
        .bind(now)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let completed_at = if let Some(inserted) = inserted {
            audit(
                tx,
                context,
                "courses.learning.lesson.completed",
                "CourseLesson",
                Some(lesson_id.into_uuid()),
                json!({"student_id": student_id}),
            )
            .await?;
            inserted
        } else {
            sqlx::query_scalar(
                "select completed_at from course_progress
                  where site_id = $1 and student_id = $2 and lesson_id = $3",
            )
            .bind(context.site_id.into_uuid())
            .bind(student_id.into_uuid())
            .bind(lesson_id.into_uuid())
            .fetch_one(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?
        };
        Ok(Progress {
            lesson_id,
            completed_at,
        })
    }
}

fn current_student(context: &SiteContext) -> Result<StudentId> {
    match &context.caller {
        mavi_core::Caller::Student { student_id, .. } => Ok(*student_id),
        _ => Err(MaviError::Unauthenticated),
    }
}

fn learning_course_from_row(row: &sqlx::postgres::PgRow) -> Result<LearningCourse> {
    Ok(LearningCourse {
        course_id: CourseId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
        title: row.try_get("title").map_err(|_| MaviError::Internal)?,
        about: row.try_get("about").map_err(|_| MaviError::Internal)?,
        state: CourseState::parse(
            &row.try_get::<String, _>("state")
                .map_err(|_| MaviError::Internal)?,
        )?,
        completed_lessons: row
            .try_get("completed_lessons")
            .map_err(|_| MaviError::Internal)?,
        total_lessons: row
            .try_get("total_lessons")
            .map_err(|_| MaviError::Internal)?,
        enrolled_at: row
            .try_get("enrolled_at")
            .map_err(|_| MaviError::Internal)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learning_contract_is_student_only_and_cursor_based() {
        let contract = serde_json::to_string(&api()).expect("contract");
        assert!(contract.contains("authentication\":\"student\""));
        assert!(contract.contains("learning.lesson.done"));
        assert!(!contract.contains("offset"));
    }
}
