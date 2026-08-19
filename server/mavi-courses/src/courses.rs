use chrono::{DateTime, Utc};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, CourseId, ErrorCode, FileId, LessonId, MaviError, ModuleId, Page,
    PageRequest, Result, SiteContext,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::{CoursesService, audit, decode_cursor, encode_cursor};

pub const COURSE_NOT_FOUND: &str = "course_not_found";
pub const COURSE_SLUG_INVALID: &str = "course_slug_invalid";
pub const COURSE_SLUG_TAKEN: &str = "course_slug_taken";
pub const COURSE_TITLE_INVALID: &str = "course_title_invalid";
pub const COURSE_ABOUT_INVALID: &str = "course_about_invalid";
pub const COURSE_CLOSED: &str = "course_closed";
pub const COURSE_STATE_TRANSITION_INVALID: &str = "course_state_transition_invalid";
pub const MODULE_NOT_FOUND: &str = "course_module_not_found";
pub const LESSON_NOT_FOUND: &str = "course_lesson_not_found";
pub const MODULE_TITLE_INVALID: &str = "course_module_title_invalid";
pub const LESSON_TITLE_INVALID: &str = "course_lesson_title_invalid";
pub const LESSON_BODY_INVALID: &str = "course_lesson_body_invalid";
pub const LESSON_MEDIA_INVALID: &str = "course_lesson_media_invalid";
pub const ORDER_INVALID: &str = "course_order_invalid";

const MAX_SLUG_CHARS: usize = 160;
const MAX_TITLE_CHARS: usize = 300;
const MAX_ABOUT_CHARS: usize = 10_000;
const MAX_BODY_CHARS: usize = 100_000;
const MAX_ORDER_ITEMS: usize = 1_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CourseState {
    Draft,
    Open,
    Closed,
}

impl CourseState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "draft" => Ok(Self::Draft),
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed),
            _ => Err(MaviError::Internal),
        }
    }

    #[must_use]
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Open)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCourse {
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub about: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateCourse {
    pub title: Option<String>,
    /// `None` leaves the about text untouched; `Some(None)` clears it.
    pub about: Option<Option<String>>,
    pub state: Option<CourseState>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CourseListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub state: Option<CourseState>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Course {
    pub id: CourseId,
    pub slug: String,
    pub title: String,
    pub about: Option<String>,
    pub state: CourseState,
    pub modules: Vec<Module>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CourseSummary {
    pub id: CourseId,
    pub slug: String,
    pub title: String,
    pub about: Option<String>,
    pub state: CourseState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateModule {
    pub title: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateModule {
    pub title: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Module {
    pub id: ModuleId,
    pub course_id: CourseId,
    pub title: String,
    pub position: i32,
    pub lessons: Vec<Lesson>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateLesson {
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub media_file_id: Option<FileId>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateLesson {
    pub title: Option<String>,
    pub body: Option<String>,
    pub media_file_id: Option<Option<FileId>>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct LessonListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Serialize)]
pub struct Lesson {
    pub id: LessonId,
    pub module_id: ModuleId,
    pub title: String,
    pub body: String,
    pub media_file_id: Option<FileId>,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReorderModules {
    pub order: Vec<ModuleId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReorderLessons {
    pub order: Vec<LessonId>,
}

#[must_use]
pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new(endpoints()).with_shapes(shapes())
}

#[allow(clippy::too_many_lines)]
fn endpoints() -> Vec<Endpoint> {
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
    vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/courses",
            "courses.list",
            "List site courses with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("CourseListFilter")
        .returns(200, "CourseSummaryPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/courses",
            "courses.create",
            "Create a draft course",
        )
        .account_or_assistant()
        .requires(write)
        .takes("CreateCourse")
        .returns(201, "Course")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/courses/{id}",
            "courses.read",
            "Read a course with ordered modules and lessons",
        )
        .account_or_assistant()
        .requires(view)
        .resource_scoped()
        .returns(200, "Course")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/courses/{id}",
            "courses.update",
            "Update course metadata or move its lifecycle state",
        )
        .account_or_assistant()
        .requires(write)
        .resource_scoped()
        .takes("UpdateCourse")
        .returns(200, "Course")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Put,
            "/api/v1/courses/{id}/modules/order",
            "courses.modules.reorder",
            "Replace a course module order atomically",
        )
        .account_or_assistant()
        .requires(write)
        .resource_scoped()
        .takes("ReorderModules")
        .returns(200, "Course")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/courses/{id}/modules",
            "courses.modules.create",
            "Append a module to a course",
        )
        .account_or_assistant()
        .requires(write)
        .resource_scoped()
        .takes("CreateModule")
        .returns(201, "Module")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/courses/modules/{id}",
            "courses.modules.read",
            "Read a module with ordered lessons",
        )
        .account_or_assistant()
        .requires(view)
        .resource_scoped()
        .returns(200, "Module")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/courses/modules/{id}",
            "courses.modules.update",
            "Rename a module",
        )
        .account_or_assistant()
        .requires(write)
        .resource_scoped()
        .takes("UpdateModule")
        .returns(200, "Module")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/courses/modules/{id}",
            "courses.modules.delete",
            "Delete a module and its lessons",
        )
        .account_or_assistant()
        .requires(delete)
        .resource_scoped()
        .returns(204, "Empty")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/courses/modules/{id}/lessons",
            "courses.lessons.list",
            "List lessons in a module with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .resource_scoped()
        .takes_query("LessonListFilter")
        .returns(200, "LessonPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Put,
            "/api/v1/courses/modules/{id}/lessons/order",
            "courses.lessons.reorder",
            "Replace a module lesson order atomically",
        )
        .account_or_assistant()
        .requires(write)
        .resource_scoped()
        .takes("ReorderLessons")
        .returns(200, "Module")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/courses/modules/{id}/lessons",
            "courses.lessons.create",
            "Append a lesson to a module",
        )
        .account_or_assistant()
        .requires(write)
        .resource_scoped()
        .takes("CreateLesson")
        .returns(201, "Lesson")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/courses/lessons/{id}",
            "courses.lessons.update",
            "Update lesson text or its media attachment",
        )
        .account_or_assistant()
        .requires(write)
        .resource_scoped()
        .takes("UpdateLesson")
        .returns(200, "Lesson")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/courses/lessons/{id}",
            "courses.lessons.delete",
            "Delete a lesson and its progress records",
        )
        .account_or_assistant()
        .requires(delete)
        .resource_scoped()
        .returns(204, "Empty")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
    ]
}

#[allow(clippy::too_many_lines)]
fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "CourseState",
            json!({"type": "string", "enum": ["draft", "open", "closed"]}),
        ),
        Shape::new(
            "CourseListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                "state": {"$ref": "#/components/schemas/CourseState"}
            }}),
        ),
        Shape::new(
            "CreateCourse",
            json!({"type": "object", "required": ["slug", "title"], "additionalProperties": false, "properties": {
                "slug": {"type": "string", "minLength": 1, "maxLength": MAX_SLUG_CHARS},
                "title": {"type": "string", "minLength": 1, "maxLength": MAX_TITLE_CHARS},
                "about": {"type": ["string", "null"], "maxLength": MAX_ABOUT_CHARS}
            }}),
        ),
        Shape::new(
            "UpdateCourse",
            json!({"type": "object", "additionalProperties": false, "properties": {
                "title": {"type": ["string", "null"], "maxLength": MAX_TITLE_CHARS},
                "about": {"type": ["string", "null"], "maxLength": MAX_ABOUT_CHARS},
                "state": {"oneOf": [{"$ref": "#/components/schemas/CourseState"}, {"type": "null"}]}
            }}),
        ),
        Shape::new(
            "CreateModule",
            json!({"type": "object", "required": ["title"], "additionalProperties": false, "properties": {
                "title": {"type": "string", "minLength": 1, "maxLength": MAX_TITLE_CHARS}
            }}),
        ),
        Shape::new(
            "UpdateModule",
            json!({"type": "object", "additionalProperties": false, "properties": {
                "title": {"type": ["string", "null"], "maxLength": MAX_TITLE_CHARS}
            }}),
        ),
        Shape::new(
            "CreateLesson",
            json!({"type": "object", "required": ["title"], "additionalProperties": false, "properties": {
                "title": {"type": "string", "minLength": 1, "maxLength": MAX_TITLE_CHARS},
                "body": {"type": "string", "maxLength": MAX_BODY_CHARS},
                "media_file_id": {"type": ["string", "null"], "format": "uuid"}
            }}),
        ),
        Shape::new(
            "UpdateLesson",
            json!({"type": "object", "additionalProperties": false, "properties": {
                "title": {"type": ["string", "null"], "maxLength": MAX_TITLE_CHARS},
                "body": {"type": ["string", "null"], "maxLength": MAX_BODY_CHARS},
                "media_file_id": {"type": ["string", "null"], "format": "uuid"}
            }}),
        ),
        Shape::new(
            "ReorderModules",
            json!({"type": "object", "required": ["order"], "additionalProperties": false, "properties": {
                "order": {"type": "array", "minItems": 0, "maxItems": MAX_ORDER_ITEMS, "items": {"type": "string", "format": "uuid"}}
            }}),
        ),
        Shape::new(
            "ReorderLessons",
            json!({"type": "object", "required": ["order"], "additionalProperties": false, "properties": {
                "order": {"type": "array", "minItems": 0, "maxItems": MAX_ORDER_ITEMS, "items": {"type": "string", "format": "uuid"}}
            }}),
        ),
        Shape::new(
            "LessonListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }}),
        ),
        Shape::new(
            "CourseSummary",
            json!({"type": "object", "required": ["id", "slug", "title", "about", "state", "created_at", "updated_at"], "properties": {
                "id": {"type": "string", "format": "uuid"}, "slug": {"type": "string"}, "title": {"type": "string"},
                "about": {"type": ["string", "null"]}, "state": {"$ref": "#/components/schemas/CourseState"},
                "created_at": {"type": "string", "format": "date-time"}, "updated_at": {"type": "string", "format": "date-time"}
            }}),
        ),
        Shape::new(
            "Lesson",
            json!({"type": "object", "required": ["id", "module_id", "title", "body", "media_file_id", "position", "created_at", "updated_at"], "properties": {
                "id": {"type": "string", "format": "uuid"}, "module_id": {"type": "string", "format": "uuid"},
                "title": {"type": "string"}, "body": {"type": "string"}, "media_file_id": {"type": ["string", "null"], "format": "uuid"},
                "position": {"type": "integer", "minimum": 0}, "created_at": {"type": "string", "format": "date-time"}, "updated_at": {"type": "string", "format": "date-time"}
            }}),
        ),
        Shape::new(
            "Module",
            json!({"type": "object", "required": ["id", "course_id", "title", "position", "lessons", "created_at", "updated_at"], "properties": {
                "id": {"type": "string", "format": "uuid"}, "course_id": {"type": "string", "format": "uuid"}, "title": {"type": "string"},
                "position": {"type": "integer", "minimum": 0}, "lessons": {"type": "array", "items": {"$ref": "#/components/schemas/Lesson"}},
                "created_at": {"type": "string", "format": "date-time"}, "updated_at": {"type": "string", "format": "date-time"}
            }}),
        ),
        Shape::new(
            "Course",
            json!({"type": "object", "required": ["id", "slug", "title", "about", "state", "modules", "created_at", "updated_at"], "properties": {
                "id": {"type": "string", "format": "uuid"}, "slug": {"type": "string"}, "title": {"type": "string"},
                "about": {"type": ["string", "null"]}, "state": {"$ref": "#/components/schemas/CourseState"},
                "modules": {"type": "array", "items": {"$ref": "#/components/schemas/Module"}},
                "created_at": {"type": "string", "format": "date-time"}, "updated_at": {"type": "string", "format": "date-time"}
            }}),
        ),
        Shape::new(
            "CourseSummaryPage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/CourseSummary"}},
                "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
        Shape::new(
            "LessonPage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/Lesson"}},
                "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
    ]
}

impl CoursesService {
    pub async fn list_courses(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &CourseListFilter,
    ) -> Result<Page<CourseSummary>> {
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, slug, title, about, state, created_at, updated_at
               from courses where site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        query.push(" and deleted_at is null");
        if let Some(state) = filter.state {
            query.push(" and state = ").push_bind(state.as_str());
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
            .map(summary_from_row)
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

    pub async fn get_course(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: CourseId,
    ) -> Result<Course> {
        let row = course_row(tx, context, id, false).await?;
        let mut course = course_from_row(&row)?;
        course.modules = load_modules(tx, context, id).await?;
        Ok(course)
    }

    pub async fn create_course(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateCourse,
    ) -> Result<Course> {
        let slug = validate_slug(&input.slug)?;
        let title = validate_title(&input.title, COURSE_TITLE_INVALID)?;
        let about = input
            .about
            .as_deref()
            .map(|value| validate_about(value).map(str::to_owned))
            .transpose()?;
        let id = CourseId::new();
        let row = sqlx::query(
            "insert into courses (site_id, id, slug, title, about)
             values ($1, $2, $3, $4, $5)
             returning id, slug, title, about, state, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(&slug)
        .bind(title)
        .bind(about.as_deref())
        .fetch_one(tx.conn())
        .await
        .map_err(map_write_error)?;
        let course = course_from_row(&row)?;
        audit(
            tx,
            context,
            "courses.course.created",
            "Course",
            Some(id.into_uuid()),
            json!({"slug": course.slug}),
        )
        .await?;
        Ok(course)
    }

    pub async fn update_course(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: CourseId,
        input: &UpdateCourse,
    ) -> Result<Course> {
        let current = course_row(tx, context, id, true).await?;
        let current_state = CourseState::parse(
            &current
                .try_get::<String, _>("state")
                .map_err(|_| MaviError::Internal)?,
        )?;
        if let Some(next) = input.state
            && next != current_state
            && !valid_state_transition(current_state, next)
        {
            return Err(MaviError::conflict(COURSE_STATE_TRANSITION_INVALID));
        }
        let title = input
            .title
            .as_deref()
            .map(|value| validate_title(value, COURSE_TITLE_INVALID).map(str::to_owned))
            .transpose()?;
        let about = input
            .about
            .as_ref()
            .map(|value| value.as_deref().map(validate_about).transpose())
            .transpose()?;
        if title.is_none() && input.about.is_none() && input.state.is_none() {
            return self.get_course(tx, context, id).await;
        }
        let row = sqlx::query(
            "update courses
                set title = coalesce($3, title),
                    about = case when $4 then $5 else about end,
                    state = coalesce($6, state), updated_at = clock_timestamp()
              where site_id = $1 and id = $2 and deleted_at is null
              returning id, slug, title, about, state, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(title.as_deref())
        .bind(input.about.is_some())
        .bind(about.flatten())
        .bind(input.state.map(CourseState::as_str))
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        audit(
            tx,
            context,
            "courses.course.updated",
            "Course",
            Some(id.into_uuid()),
            json!({"state": input.state}),
        )
        .await?;
        let mut course = course_from_row(&row)?;
        course.modules = load_modules(tx, context, id).await?;
        Ok(course)
    }

    pub async fn create_module(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        course_id: CourseId,
        input: &CreateModule,
    ) -> Result<Module> {
        ensure_course_editable(tx, context, course_id).await?;
        let title = validate_title(&input.title, MODULE_TITLE_INVALID)?;
        let position: i32 = sqlx::query_scalar(
            "select coalesce(max(position), -1) + 1 from course_modules
              where site_id = $1 and course_id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(course_id.into_uuid())
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let id = ModuleId::new();
        let row = sqlx::query(
            "insert into course_modules (site_id, id, course_id, title, position)
             values ($1, $2, $3, $4, $5)
             returning id, course_id, title, position, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(course_id.into_uuid())
        .bind(title)
        .bind(position)
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let module = module_from_row(&row)?;
        audit(
            tx,
            context,
            "courses.module.created",
            "CourseModule",
            Some(id.into_uuid()),
            json!({"course_id": course_id}),
        )
        .await?;
        Ok(module)
    }

    pub async fn get_module(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ModuleId,
    ) -> Result<Module> {
        let row = module_row(tx, context, id, false).await?;
        let mut module = module_from_row(&row)?;
        module.lessons = load_lessons(tx, context, id).await?;
        Ok(module)
    }

    pub async fn update_module(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ModuleId,
        input: &UpdateModule,
    ) -> Result<Module> {
        let current = module_row(tx, context, id, false).await?;
        let course_id = CourseId::from_uuid(
            current
                .try_get("course_id")
                .map_err(|_| MaviError::Internal)?,
        );
        ensure_course_editable(tx, context, course_id).await?;
        let _ = module_row(tx, context, id, false).await?;
        if input.title.is_none() {
            return self.get_module(tx, context, id).await;
        }
        let title = validate_title(
            input.title.as_deref().unwrap_or_default(),
            MODULE_TITLE_INVALID,
        )?;
        sqlx::query(
            "update course_modules set title = $3, updated_at = clock_timestamp()
              where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(title)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        audit(
            tx,
            context,
            "courses.module.updated",
            "CourseModule",
            Some(id.into_uuid()),
            json!({}),
        )
        .await?;
        self.get_module(tx, context, id).await
    }

    pub async fn delete_module(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: ModuleId,
    ) -> Result<()> {
        let row = module_row(tx, context, id, false).await?;
        let course_id =
            CourseId::from_uuid(row.try_get("course_id").map_err(|_| MaviError::Internal)?);
        ensure_course_editable(tx, context, course_id).await?;
        let _ = module_row(tx, context, id, false).await?;
        let changed = sqlx::query("delete from course_modules where site_id = $1 and id = $2")
            .bind(context.site_id.into_uuid())
            .bind(id.into_uuid())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        if changed.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: MODULE_NOT_FOUND,
            });
        }
        audit(
            tx,
            context,
            "courses.module.deleted",
            "CourseModule",
            Some(id.into_uuid()),
            json!({"course_id": course_id}),
        )
        .await
    }

    pub async fn reorder_modules(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        course_id: CourseId,
        input: &ReorderModules,
    ) -> Result<Course> {
        ensure_course_editable(tx, context, course_id).await?;
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "select id from course_modules where site_id = $1 and course_id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(course_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let order = input
            .order
            .iter()
            .map(|id| id.into_uuid())
            .collect::<Vec<_>>();
        validate_order(&ids, &order)?;
        for (position, id) in order.into_iter().enumerate() {
            sqlx::query(
                "update course_modules set position = $3, updated_at = clock_timestamp()
                  where site_id = $1 and id = $2",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .bind(i32::try_from(position).map_err(|_| MaviError::Internal)?)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        audit(
            tx,
            context,
            "courses.modules.reordered",
            "Course",
            Some(course_id.into_uuid()),
            json!({"count": input.order.len()}),
        )
        .await?;
        self.get_course(tx, context, course_id).await
    }

    pub async fn list_lessons(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        module_id: ModuleId,
        filter: &LessonListFilter,
    ) -> Result<Page<Lesson>> {
        let _ = module_row(tx, context, module_id, false).await?;
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, module_id, title, body, media_file_id, position, created_at, updated_at
               from course_lessons where site_id = ",
        );
        query
            .push_bind(context.site_id.into_uuid())
            .push(" and module_id = ")
            .push_bind(module_id.into_uuid());
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
            .map(lesson_from_row)
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

    pub async fn create_lesson(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        module_id: ModuleId,
        input: &CreateLesson,
    ) -> Result<Lesson> {
        let module = module_row(tx, context, module_id, false).await?;
        let course_id = CourseId::from_uuid(
            module
                .try_get("course_id")
                .map_err(|_| MaviError::Internal)?,
        );
        ensure_course_editable(tx, context, course_id).await?;
        let _ = module_row(tx, context, module_id, false).await?;
        let title = validate_title(&input.title, LESSON_TITLE_INVALID)?;
        validate_body(&input.body)?;
        let position: i32 = sqlx::query_scalar(
            "select coalesce(max(position), -1) + 1 from course_lessons
              where site_id = $1 and module_id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(module_id.into_uuid())
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let id = LessonId::new();
        let row = sqlx::query(
            "insert into course_lessons
                (site_id, id, module_id, title, body, media_file_id, position)
             values ($1, $2, $3, $4, $5, $6, $7)
             returning id, module_id, title, body, media_file_id, position, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(module_id.into_uuid())
        .bind(title)
        .bind(&input.body)
        .bind(input.media_file_id.map(FileId::into_uuid))
        .bind(position)
        .fetch_one(tx.conn())
        .await
        .map_err(map_write_error)?;
        let lesson = lesson_from_row(&row)?;
        audit(
            tx,
            context,
            "courses.lesson.created",
            "CourseLesson",
            Some(id.into_uuid()),
            json!({"module_id": module_id}),
        )
        .await?;
        Ok(lesson)
    }

    pub async fn update_lesson(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: LessonId,
        input: &UpdateLesson,
    ) -> Result<Lesson> {
        let current = lesson_row(tx, context, id, false).await?;
        let module_id = ModuleId::from_uuid(
            current
                .try_get("module_id")
                .map_err(|_| MaviError::Internal)?,
        );
        let module = module_row(tx, context, module_id, false).await?;
        let course_id = CourseId::from_uuid(
            module
                .try_get("course_id")
                .map_err(|_| MaviError::Internal)?,
        );
        ensure_course_editable(tx, context, course_id).await?;
        let current = lesson_row(tx, context, id, false).await?;
        let title = input
            .title
            .as_deref()
            .map(|value| validate_title(value, LESSON_TITLE_INVALID).map(str::to_owned))
            .transpose()?;
        if let Some(body) = &input.body {
            validate_body(body)?;
        }
        if title.is_none() && input.body.is_none() && input.media_file_id.is_none() {
            return lesson_from_row(&current);
        }
        let row = sqlx::query(
            "update course_lessons
                set title = coalesce($3, title), body = coalesce($4, body),
                    media_file_id = case when $5 then $6 else media_file_id end,
                    updated_at = clock_timestamp()
              where site_id = $1 and id = $2
              returning id, module_id, title, body, media_file_id, position, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(title.as_deref())
        .bind(input.body.as_deref())
        .bind(input.media_file_id.is_some())
        .bind(input.media_file_id.flatten().map(FileId::into_uuid))
        .fetch_one(tx.conn())
        .await
        .map_err(map_write_error)?;
        audit(
            tx,
            context,
            "courses.lesson.updated",
            "CourseLesson",
            Some(id.into_uuid()),
            json!({"module_id": module_id}),
        )
        .await?;
        lesson_from_row(&row)
    }

    pub async fn delete_lesson(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: LessonId,
    ) -> Result<()> {
        let row = lesson_row(tx, context, id, false).await?;
        let module_id =
            ModuleId::from_uuid(row.try_get("module_id").map_err(|_| MaviError::Internal)?);
        let module = module_row(tx, context, module_id, false).await?;
        let course_id = CourseId::from_uuid(
            module
                .try_get("course_id")
                .map_err(|_| MaviError::Internal)?,
        );
        ensure_course_editable(tx, context, course_id).await?;
        let _ = lesson_row(tx, context, id, false).await?;
        let changed = sqlx::query("delete from course_lessons where site_id = $1 and id = $2")
            .bind(context.site_id.into_uuid())
            .bind(id.into_uuid())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        if changed.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: LESSON_NOT_FOUND,
            });
        }
        audit(
            tx,
            context,
            "courses.lesson.deleted",
            "CourseLesson",
            Some(id.into_uuid()),
            json!({"module_id": module_id}),
        )
        .await
    }

    pub async fn reorder_lessons(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        module_id: ModuleId,
        input: &ReorderLessons,
    ) -> Result<Module> {
        let module = module_row(tx, context, module_id, false).await?;
        let course_id = CourseId::from_uuid(
            module
                .try_get("course_id")
                .map_err(|_| MaviError::Internal)?,
        );
        ensure_course_editable(tx, context, course_id).await?;
        let _ = module_row(tx, context, module_id, false).await?;
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "select id from course_lessons where site_id = $1 and module_id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(module_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let order = input
            .order
            .iter()
            .map(|id| id.into_uuid())
            .collect::<Vec<_>>();
        validate_order(&ids, &order)?;
        for (position, id) in order.into_iter().enumerate() {
            sqlx::query(
                "update course_lessons set position = $3, updated_at = clock_timestamp()
                  where site_id = $1 and id = $2",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .bind(i32::try_from(position).map_err(|_| MaviError::Internal)?)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        audit(
            tx,
            context,
            "courses.lessons.reordered",
            "CourseModule",
            Some(module_id.into_uuid()),
            json!({"count": input.order.len()}),
        )
        .await?;
        self.get_module(tx, context, module_id).await
    }
}

async fn course_row(
    tx: &mut SiteTx,
    context: &SiteContext,
    id: CourseId,
    lock: bool,
) -> Result<sqlx::postgres::PgRow> {
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "select id, slug, title, about, state, created_at, updated_at
           from courses where site_id = ",
    );
    query
        .push_bind(context.site_id.into_uuid())
        .push(" and id = ")
        .push_bind(id.into_uuid())
        .push(" and deleted_at is null");
    if lock {
        query.push(" for update");
    }
    query
        .build()
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: COURSE_NOT_FOUND,
        })
}

async fn module_row(
    tx: &mut SiteTx,
    context: &SiteContext,
    id: ModuleId,
    lock: bool,
) -> Result<sqlx::postgres::PgRow> {
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "select id, course_id, title, position, created_at, updated_at
           from course_modules where site_id = ",
    );
    query
        .push_bind(context.site_id.into_uuid())
        .push(" and id = ")
        .push_bind(id.into_uuid());
    if lock {
        query.push(" for update");
    }
    query
        .build()
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: MODULE_NOT_FOUND,
        })
}

async fn lesson_row(
    tx: &mut SiteTx,
    context: &SiteContext,
    id: LessonId,
    lock: bool,
) -> Result<sqlx::postgres::PgRow> {
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "select id, module_id, title, body, media_file_id, position, created_at, updated_at
           from course_lessons where site_id = ",
    );
    query
        .push_bind(context.site_id.into_uuid())
        .push(" and id = ")
        .push_bind(id.into_uuid());
    if lock {
        query.push(" for update");
    }
    query
        .build()
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: LESSON_NOT_FOUND,
        })
}

async fn load_modules(
    tx: &mut SiteTx,
    context: &SiteContext,
    course_id: CourseId,
) -> Result<Vec<Module>> {
    let rows = sqlx::query(
        "select id, course_id, title, position, created_at, updated_at
           from course_modules where site_id = $1 and course_id = $2
          order by position asc, id asc",
    )
    .bind(context.site_id.into_uuid())
    .bind(course_id.into_uuid())
    .fetch_all(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    let mut modules = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut module = module_from_row(row)?;
        module.lessons = load_lessons(tx, context, module.id).await?;
        modules.push(module);
    }
    Ok(modules)
}

async fn load_lessons(
    tx: &mut SiteTx,
    context: &SiteContext,
    module_id: ModuleId,
) -> Result<Vec<Lesson>> {
    let rows = sqlx::query(
        "select id, module_id, title, body, media_file_id, position, created_at, updated_at
           from course_lessons where site_id = $1 and module_id = $2
          order by position asc, id asc",
    )
    .bind(context.site_id.into_uuid())
    .bind(module_id.into_uuid())
    .fetch_all(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    rows.iter().map(lesson_from_row).collect()
}

async fn ensure_course_editable(
    tx: &mut SiteTx,
    context: &SiteContext,
    course_id: CourseId,
) -> Result<CourseState> {
    let state: String = sqlx::query_scalar(
        "select state from courses where site_id = $1 and id = $2 and deleted_at is null for update",
    )
    .bind(context.site_id.into_uuid())
    .bind(course_id.into_uuid())
    .fetch_optional(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?
    .ok_or(MaviError::NotFound {
        resource: COURSE_NOT_FOUND,
    })?;
    let state = CourseState::parse(&state)?;
    if state == CourseState::Closed {
        return Err(MaviError::conflict(COURSE_CLOSED));
    }
    Ok(state)
}

fn valid_state_transition(from: CourseState, to: CourseState) -> bool {
    matches!(
        (from, to),
        (CourseState::Draft, CourseState::Open) | (CourseState::Open, CourseState::Closed)
    )
}

fn validate_order(current: &[Uuid], requested: &[Uuid]) -> Result<()> {
    if requested.len() > MAX_ORDER_ITEMS || requested.len() != current.len() {
        return Err(MaviError::validation(ORDER_INVALID));
    }
    let mut current = current.to_vec();
    let mut requested = requested.to_vec();
    current.sort_unstable();
    requested.sort_unstable();
    if current != requested {
        return Err(MaviError::validation(ORDER_INVALID));
    }
    Ok(())
}

fn validate_slug(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_SLUG_CHARS
        || value.starts_with('-')
        || value.ends_with('-')
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(MaviError::validation_field(COURSE_SLUG_INVALID, "slug"));
    }
    Ok(value.to_owned())
}

fn validate_title<'a>(value: &'a str, code: &'static str) -> Result<&'a str> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_TITLE_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(MaviError::validation(code));
    }
    Ok(value)
}

fn validate_about(value: &str) -> Result<&str> {
    if value.chars().count() > MAX_ABOUT_CHARS || value.contains('\0') {
        return Err(MaviError::validation(COURSE_ABOUT_INVALID));
    }
    Ok(value)
}

fn validate_body(value: &str) -> Result<()> {
    if value.chars().count() > MAX_BODY_CHARS || value.contains('\0') {
        return Err(MaviError::validation(LESSON_BODY_INVALID));
    }
    Ok(())
}

fn summary_from_row(row: &sqlx::postgres::PgRow) -> Result<CourseSummary> {
    let course = course_from_row(row)?;
    Ok(CourseSummary {
        id: course.id,
        slug: course.slug,
        title: course.title,
        about: course.about,
        state: course.state,
        created_at: course.created_at,
        updated_at: course.updated_at,
    })
}

fn course_from_row(row: &sqlx::postgres::PgRow) -> Result<Course> {
    Ok(Course {
        id: CourseId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
        title: row.try_get("title").map_err(|_| MaviError::Internal)?,
        about: row.try_get("about").map_err(|_| MaviError::Internal)?,
        state: CourseState::parse(
            &row.try_get::<String, _>("state")
                .map_err(|_| MaviError::Internal)?,
        )?,
        modules: Vec::new(),
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn module_from_row(row: &sqlx::postgres::PgRow) -> Result<Module> {
    Ok(Module {
        id: ModuleId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        course_id: CourseId::from_uuid(row.try_get("course_id").map_err(|_| MaviError::Internal)?),
        title: row.try_get("title").map_err(|_| MaviError::Internal)?,
        position: row.try_get("position").map_err(|_| MaviError::Internal)?,
        lessons: Vec::new(),
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

pub(crate) fn lesson_from_row(row: &sqlx::postgres::PgRow) -> Result<Lesson> {
    Ok(Lesson {
        id: LessonId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        module_id: ModuleId::from_uuid(row.try_get("module_id").map_err(|_| MaviError::Internal)?),
        title: row.try_get("title").map_err(|_| MaviError::Internal)?,
        body: row.try_get("body").map_err(|_| MaviError::Internal)?,
        media_file_id: row
            .try_get::<Option<Uuid>, _>("media_file_id")
            .map_err(|_| MaviError::Internal)?
            .map(FileId::from_uuid),
        position: row.try_get("position").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn map_write_error(error: sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error {
        match database.constraint() {
            Some("courses_site_slug_active") => return MaviError::conflict(COURSE_SLUG_TAKEN),
            Some("course_lessons_site_media_file_id_fkey") => {
                return MaviError::validation(LESSON_MEDIA_INVALID);
            }
            Some("course_lessons_site_position" | "course_modules_site_position") => {
                return MaviError::conflict(ORDER_INVALID);
            }
            _ => {}
        }
    }
    MaviError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn course_state_only_moves_forward() {
        assert!(valid_state_transition(
            CourseState::Draft,
            CourseState::Open
        ));
        assert!(valid_state_transition(
            CourseState::Open,
            CourseState::Closed
        ));
        assert!(!valid_state_transition(
            CourseState::Closed,
            CourseState::Open
        ));
        assert!(!valid_state_transition(
            CourseState::Draft,
            CourseState::Closed
        ));
    }

    #[test]
    fn reorders_require_the_exact_same_ids() {
        let first = Uuid::from_u128(1);
        let second = Uuid::from_u128(2);
        assert!(validate_order(&[first, second], &[second, first]).is_ok());
        assert!(validate_order(&[first, second], &[first, first]).is_err());
        assert!(validate_order(&[first, second], &[first]).is_err());
    }

    #[test]
    fn course_contract_is_cursor_only_and_closed_content_is_explicit() {
        let contract = serde_json::to_string(&api()).expect("contract");
        assert!(contract.contains("courses.modules.reorder"));
        assert!(contract.contains("media_file_id"));
        assert!(!contract.contains("offset"));
    }
}
