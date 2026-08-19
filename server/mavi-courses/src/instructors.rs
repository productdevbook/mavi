use chrono::{DateTime, Utc};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, CourseId, ErrorCode, Grant, Grants, MaviError, Page, PageRequest, PersonId,
    Result, SiteContext,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;

use crate::{CoursesService, audit, decode_cursor, encode_cursor};

pub const COURSE_INSTRUCTOR_NOT_FOUND: &str = "course_instructor_not_found";
pub const COURSE_INSTRUCTOR_PERSON_NOT_FOUND: &str = "course_instructor_person_not_found";
pub const COURSE_INSTRUCTOR_GRANTS_INVALID: &str = "course_instructor_grants_invalid";

/// The only site grants that can be delegated to an instructor on one course.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CourseInstructorGrant {
    View,
    Write,
    Delete,
}

impl CourseInstructorGrant {
    const fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Write => "write",
            Self::Delete => "delete",
        }
    }

    const fn as_action(self) -> Action {
        match self {
            Self::View => Action::View,
            Self::Write => Action::Write,
            Self::Delete => Action::Delete,
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "view" => Ok(Self::View),
            "write" => Ok(Self::Write),
            "delete" => Ok(Self::Delete),
            _ => Err(MaviError::Internal),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceCourseInstructor {
    pub grants: Vec<CourseInstructorGrant>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CourseInstructorListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Serialize)]
pub struct CourseInstructor {
    pub course_id: CourseId,
    pub person_id: PersonId,
    pub grants: Vec<CourseInstructorGrant>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

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
    mavi_contract::Api::new(vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/courses/{course_id}/instructors",
            "courses.instructors.list",
            "List resource-scoped instructors for a course",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("CourseInstructorListFilter")
        .returns(200, "CourseInstructorPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Put,
            "/api/v1/courses/{course_id}/instructors/{person_id}",
            "courses.instructors.replace",
            "Replace one instructor's resource-scoped course grants",
        )
        .account_or_assistant()
        .requires(write)
        .takes("ReplaceCourseInstructor")
        .returns(200, "CourseInstructor")
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
            "/api/v1/courses/{course_id}/instructors/{person_id}",
            "courses.instructors.delete",
            "Remove one instructor from a course",
        )
        .account_or_assistant()
        .requires(write)
        .returns(204, "Empty")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
    ])
    .with_shapes([
        Shape::new(
            "CourseInstructorGrant",
            json!({"type": "string", "enum": ["view", "write", "delete"]}),
        ),
        Shape::new(
            "ReplaceCourseInstructor",
            json!({
                "type": "object",
                "required": ["grants"],
                "additionalProperties": false,
                "properties": {
                    "grants": {"type": "array", "minItems": 1, "maxItems": 3, "items": {"$ref": "#/components/schemas/CourseInstructorGrant"}}
                }
            }),
        ),
        Shape::new(
            "CourseInstructorListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }}),
        ),
        Shape::new(
            "CourseInstructor",
            json!({
                "type": "object",
                "required": ["course_id", "person_id", "grants", "created_at", "updated_at"],
                "properties": {
                    "course_id": {"type": "string", "format": "uuid"},
                    "person_id": {"type": "string", "format": "uuid"},
                    "grants": {"type": "array", "items": {"$ref": "#/components/schemas/CourseInstructorGrant"}},
                    "created_at": {"type": "string", "format": "date-time"},
                    "updated_at": {"type": "string", "format": "date-time"}
                }
            }),
        ),
        Shape::new(
            "CourseInstructorPage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/CourseInstructor"}},
                "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
    ])
}

impl CoursesService {
    pub async fn list_instructors(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        course_id: CourseId,
        filter: &CourseInstructorListFilter,
    ) -> Result<Page<CourseInstructor>> {
        ensure_course(tx, context, course_id).await?;
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select course_id, person_id, grants, created_at, updated_at
               from course_instructors where site_id = ",
        );
        query
            .push_bind(context.site_id.into_uuid())
            .push(" and course_id = ")
            .push_bind(course_id.into_uuid());
        if let Some(after) = after {
            query
                .push(" and (created_at, person_id) > (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        let rows = query
            .push(" order by created_at asc, person_id asc limit ")
            .push_bind(limit + 1)
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows
            .iter()
            .map(instructor_from_row)
            .collect::<Result<Vec<_>>>()?;
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let last = items.get(limit - 1).ok_or(MaviError::Internal)?;
            Some(encode_cursor(last.created_at, last.person_id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn replace_instructor(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        course_id: CourseId,
        person_id: PersonId,
        input: &ReplaceCourseInstructor,
    ) -> Result<CourseInstructor> {
        ensure_course(tx, context, course_id).await?;
        let grants = normalize_grants(&input.grants)?;
        let person_exists = sqlx::query_scalar::<_, bool>(
            "select exists(
                select 1 from people where site_id = $1 and id = $2 and status = 'active'
            )",
        )
        .bind(context.site_id.into_uuid())
        .bind(person_id.into_uuid())
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if !person_exists {
            return Err(MaviError::NotFound {
                resource: COURSE_INSTRUCTOR_PERSON_NOT_FOUND,
            });
        }
        let grant_values: Vec<&str> = grants.iter().map(|grant| grant.as_str()).collect();
        let row = sqlx::query(
            "insert into course_instructors (site_id, course_id, person_id, grants)
             values ($1, $2, $3, $4)
             on conflict (site_id, course_id, person_id) do update
                set grants = excluded.grants, updated_at = clock_timestamp()
             returning course_id, person_id, grants, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(course_id.into_uuid())
        .bind(person_id.into_uuid())
        .bind(&grant_values)
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let instructor = instructor_from_row(&row)?;
        audit(
            tx,
            context,
            "courses.instructor.replaced",
            "CourseInstructor",
            Some(person_id.into_uuid()),
            json!({"course_id": course_id, "grants": &instructor.grants}),
        )
        .await?;
        Ok(instructor)
    }

    pub async fn remove_instructor(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        course_id: CourseId,
        person_id: PersonId,
    ) -> Result<()> {
        ensure_course(tx, context, course_id).await?;
        let changed = sqlx::query(
            "delete from course_instructors where site_id = $1 and course_id = $2 and person_id = $3",
        )
        .bind(context.site_id.into_uuid())
        .bind(course_id.into_uuid())
        .bind(person_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if changed.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: COURSE_INSTRUCTOR_NOT_FOUND,
            });
        }
        audit(
            tx,
            context,
            "courses.instructor.removed",
            "CourseInstructor",
            Some(person_id.into_uuid()),
            json!({"course_id": course_id}),
        )
        .await
    }

    /// Returns resource grants for an authenticated panel account. API keys
    /// deliberately do not inherit a person's course assignments: delegation
    /// remains explicit through the key's own site-wide grants.
    pub async fn instructor_grants(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        course_id: CourseId,
    ) -> Result<Grants> {
        let person_id = match &context.caller {
            mavi_core::Caller::Account { person_id, .. } => Some(*person_id),
            _ => None,
        };
        let Some(person_id) = person_id else {
            return Ok(Grants::default());
        };
        let grants = sqlx::query_scalar::<_, Vec<String>>(
            "select grants from course_instructors
              where site_id = $1 and course_id = $2 and person_id = $3",
        )
        .bind(context.site_id.into_uuid())
        .bind(course_id.into_uuid())
        .bind(person_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .unwrap_or_default();
        Ok(Grants::new(
            grants
                .iter()
                .map(|grant| {
                    CourseInstructorGrant::parse(grant)
                        .map(|grant| Grant::new(Capability::Courses, grant.as_action()))
                })
                .collect::<Result<Vec<_>>>()?,
        ))
    }

    pub async fn course_id_for_module(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        module_id: mavi_core::ModuleId,
    ) -> Result<CourseId> {
        sqlx::query_scalar("select course_id from course_modules where site_id = $1 and id = $2")
            .bind(context.site_id.into_uuid())
            .bind(module_id.into_uuid())
            .fetch_optional(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?
            .map(CourseId::from_uuid)
            .ok_or(MaviError::NotFound {
                resource: crate::courses::MODULE_NOT_FOUND,
            })
    }

    pub async fn course_id_for_lesson(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        lesson_id: mavi_core::LessonId,
    ) -> Result<CourseId> {
        sqlx::query_scalar(
            "select m.course_id from course_lessons l
              join course_modules m on m.site_id = l.site_id and m.id = l.module_id
              where l.site_id = $1 and l.id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(lesson_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .map(CourseId::from_uuid)
        .ok_or(MaviError::NotFound {
            resource: crate::courses::LESSON_NOT_FOUND,
        })
    }

    pub async fn course_id_for_enrollment(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        enrollment_id: mavi_core::EnrollmentId,
    ) -> Result<CourseId> {
        sqlx::query_scalar(
            "select course_id from course_enrollments where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(enrollment_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .map(CourseId::from_uuid)
        .ok_or(MaviError::NotFound {
            resource: "course_enrollment_not_found",
        })
    }
}

async fn ensure_course(tx: &mut SiteTx, context: &SiteContext, course_id: CourseId) -> Result<()> {
    let exists = sqlx::query_scalar::<_, bool>(
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
            resource: crate::courses::COURSE_NOT_FOUND,
        })
    }
}

fn normalize_grants(input: &[CourseInstructorGrant]) -> Result<Vec<CourseInstructorGrant>> {
    if input.is_empty() || input.len() > 3 {
        return Err(MaviError::validation(COURSE_INSTRUCTOR_GRANTS_INVALID));
    }
    let mut grants = input.to_vec();
    grants.sort_unstable();
    grants.dedup();
    if grants.len() != input.len() {
        return Err(MaviError::validation(COURSE_INSTRUCTOR_GRANTS_INVALID));
    }
    Ok(grants)
}

fn instructor_from_row(row: &sqlx::postgres::PgRow) -> Result<CourseInstructor> {
    let grant_names: Vec<String> = row.try_get("grants").map_err(|_| MaviError::Internal)?;
    let mut grants = grant_names
        .iter()
        .map(|grant| CourseInstructorGrant::parse(grant))
        .collect::<Result<Vec<_>>>()?;
    grants.sort_unstable();
    Ok(CourseInstructor {
        course_id: CourseId::from_uuid(row.try_get("course_id").map_err(|_| MaviError::Internal)?),
        person_id: PersonId::from_uuid(row.try_get("person_id").map_err(|_| MaviError::Internal)?),
        grants,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructor_grants_are_unique_and_canonicalized() {
        assert_eq!(
            normalize_grants(&[
                CourseInstructorGrant::Write,
                CourseInstructorGrant::View,
                CourseInstructorGrant::Delete,
            ])
            .expect("grants"),
            vec![
                CourseInstructorGrant::View,
                CourseInstructorGrant::Write,
                CourseInstructorGrant::Delete,
            ]
        );
        assert!(normalize_grants(&[]).is_err());
        assert!(
            normalize_grants(&[CourseInstructorGrant::View, CourseInstructorGrant::View,]).is_err()
        );
    }

    #[test]
    fn instructor_api_is_cursor_only_and_validated() {
        let api = api();
        api.validate().expect("course instructor API");
        let json = serde_json::to_string(&api).expect("contract");
        assert!(json.contains("courses.instructors.replace"));
        assert!(json.contains("CourseInstructorPage"));
        assert!(!json.contains("offset"));
        assert!(!json.contains("page_number"));
    }
}
