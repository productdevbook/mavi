//! Courses, and the people on them.
//!
//! A course holds modules and lessons; a lesson plays a video the site
//! uploaded. Somebody is put on it for as long as the site says, and access
//! sold for ninety days stops opening the course after ninety days.
//!
//! A student is not a panel account: they sign in at the site's own front,
//! hold no grants at all, and reach nothing in the panel.
use axum::Json;
use axum::extract::{Path, Query as HttpQuery, State as Injected};
use axum::http::StatusCode;
use axum::http::header::SET_COOKIE;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Auditable, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::db::{Db, Tx};
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{
    AppState, Audience, Caller, Endpoint, Guard, RatePolicy, STUDENT_COOKIE, SignedInStudent,
};
use crate::kernel::page::{Page, Query, older_than};
use crate::kernel::ratelimit::Limit;
use crate::kernel::say;
use crate::kernel::secret::{Secret, Shown};
use crate::kernel::types::{Email, Slug, Title};
use crate::kernel::wiring::Answers;
use crate::kernel::{password, token};

const SESSION_DAYS: i64 = 30;

const SIGN_IN_LIMIT: Limit = Limit::new(5, 60);

fn courses(access: Access) -> Needs {
    Needs::new(Capability::Courses, access)
}

#[must_use]
#[expect(clippy::too_many_lines, reason = "one list of what is served")]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/courses",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Course>>(),
        Endpoint::post(
            "/api/courses",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::Write)),
                rate: RatePolicy::None,
            },
            create,
        )
        .takes::<NewCourse>()
        .gives::<Course>(),
        Endpoint::get(
            "/api/courses/{id}/students",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::View)),
                rate: RatePolicy::None,
            },
            enrolled,
        )
        .gives::<Page<OnCourse>>(),
        Endpoint::post(
            "/api/courses/{id}/students",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::Write)),
                rate: RatePolicy::None,
            },
            enrol,
        )
        .takes::<Enrolling>()
        .gives::<Enrolled>(),
        Endpoint::get(
            "/api/students",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::View)),
                rate: RatePolicy::None,
            },
            students,
        )
        .gives::<Page<Student>>(),
        Endpoint::patch(
            "/api/students/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::Write)),
                rate: RatePolicy::None,
            },
            change_student,
        )
        .takes::<StudentChanges>()
        .gives::<Student>(),
        Endpoint::get(
            "/api/courses/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::View)),
                rate: RatePolicy::None,
            },
            whole_course,
        )
        .gives::<Curriculum>(),
        Endpoint::patch(
            "/api/courses/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::Write)),
                rate: RatePolicy::None,
            },
            change_course,
        )
        .takes::<CourseChanges>()
        .gives::<Course>(),
        Endpoint::post(
            "/api/courses/{id}/modules",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::Write)),
                rate: RatePolicy::None,
            },
            add_a_module,
        )
        .takes::<NewModule>()
        .gives::<Module>(),
        Endpoint::patch(
            "/api/modules/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::Write)),
                rate: RatePolicy::None,
            },
            change_module,
        )
        .takes::<ModuleChanges>()
        .gives::<Module>(),
        Endpoint::delete(
            "/api/modules/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::Delete)),
                rate: RatePolicy::None,
            },
            remove_module,
        ),
        Endpoint::post(
            "/api/modules/{id}/lessons",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::Write)),
                rate: RatePolicy::None,
            },
            add_a_lesson,
        )
        .takes::<NewLesson>()
        .gives::<Lesson>(),
        Endpoint::patch(
            "/api/lessons/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::Write)),
                rate: RatePolicy::None,
            },
            change_lesson,
        )
        .takes::<LessonChanges>()
        .gives::<Lesson>(),
        Endpoint::delete(
            "/api/lessons/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::Delete)),
                rate: RatePolicy::None,
            },
            remove_lesson,
        ),
        Endpoint::get(
            "/api/students/{id}/enrolments",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::View)),
                rate: RatePolicy::None,
            },
            enrolments_of,
        )
        .gives::<Vec<Enrolment>>(),
        Endpoint::patch(
            "/api/enrolments/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::Write)),
                rate: RatePolicy::None,
            },
            change_enrolment,
        )
        .takes::<EnrolmentChanges>()
        .gives::<Enrolment>(),
        Endpoint::delete(
            "/api/enrolments/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(courses(Access::Delete)),
                rate: RatePolicy::None,
            },
            revoke,
        ),
        Endpoint::post(
            "/api/learn/session",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(SIGN_IN_LIMIT),
            },
            sign_in,
        )
        .takes::<StudentCredentials>(),
        Endpoint::get(
            "/api/learn/courses",
            Guard {
                audience: Audience::Student,
                needs: None,
                rate: RatePolicy::None,
            },
            mine,
        )
        .gives::<Page<Course>>(),
        Endpoint::get(
            "/api/learn/courses/{id}",
            Guard {
                audience: Audience::Student,
                needs: None,
                rate: RatePolicy::None,
            },
            curriculum,
        )
        .gives::<Curriculum>(),
        Endpoint::delete(
            "/api/learn/session",
            Guard {
                audience: Audience::Student,
                needs: None,
                rate: RatePolicy::None,
            },
            sign_out,
        ),
        Endpoint::get(
            "/api/learn/me",
            Guard {
                audience: Audience::Student,
                needs: None,
                rate: RatePolicy::None,
            },
            student_me,
        )
        .gives::<Learner>(),
        Endpoint::get(
            "/api/learn/lessons/{id}",
            Guard {
                audience: Audience::Student,
                needs: None,
                rate: RatePolicy::None,
            },
            watch,
        )
        .gives::<Watching>(),
        Endpoint::get(
            "/api/learn/videos/{id}",
            Guard {
                audience: Audience::Student,
                needs: None,
                rate: RatePolicy::None,
            },
            play,
        ),
        Endpoint::post(
            "/api/learn/lessons/{id}/done",
            Guard {
                audience: Audience::Student,
                needs: None,
                rate: RatePolicy::None,
            },
            mark_done,
        ),
    ]
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "course_state", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum CourseState {
    Draft,
    Open,
    Closed,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "student_state", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum StudentState {
    Invited,
    Active,
    Suspended,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Course {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    pub state: CourseState,
    pub created_at: DateTime<Utc>,
}

impl Auditable for Course {
    const SUBJECT: &'static str = "course";

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    fn summary(&self) -> serde_json::Value {
        serde_json::json!({ "slug": self.slug, "title": self.title, "state": self.state })
    }
}

#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Curriculum {
    pub course: Course,
    pub modules: Vec<Module>,
}

#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Module {
    pub id: Uuid,
    pub title: String,
    pub position: i32,
    pub lessons: Vec<Lesson>,
}

#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Lesson {
    pub id: Uuid,
    pub title: String,
    pub position: i32,
    /// What a student watches, where a lesson has one.
    pub video_id: Option<Uuid>,
    /// Whether this student has finished it. Always false when a panel is
    /// asking: it is a fact about a person, and nobody is one here.
    pub done: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CourseChanges {
    pub title: Option<Title>,
    pub summary: Option<String>,
    /// `draft` while it is being written, `open` once people may be on it.
    pub state: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewModule {
    pub title: Title,
    /// Where it sits. Left out, it goes last.
    #[serde(default)]
    pub position: Option<i32>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleChanges {
    pub title: Option<Title>,
    pub position: Option<i32>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewLesson {
    pub title: Title,
    #[serde(default)]
    pub position: Option<i32>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub video_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LessonChanges {
    pub title: Option<Title>,
    pub position: Option<i32>,
    pub body: Option<String>,
    pub video_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewCourse {
    pub slug: Slug,
    pub title: Title,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Enrolling {
    /// How long they may watch it. Left out is for ever.
    #[serde(default)]
    pub days: Option<i32>,
    pub email: Email,
    pub name: Title,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Enrolled {
    pub student_id: Uuid,
    /// A `student.invited` letter goes to their address as well, but this is
    /// also handed back: the panel is signed in as whoever is doing the
    /// enrolling, not as the student, so there is no other screen this could
    /// be read from if the letter never arrives.
    pub token: Shown,
}

/// What somebody is on, and until when.
#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Enrolment {
    pub id: Uuid,
    pub course_id: Uuid,
    pub course: String,
    /// When it stops opening the course. Null is for ever.
    pub ends_at: Option<DateTime<Utc>>,
    /// `waiting` where the course is not open yet, `open` while it runs,
    /// `ended` once it has. A string rather than one of the database's own
    /// kinds because it is worked out from three things rather than stored:
    /// what the course is, when access ends, and what the moment is now.
    pub state: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EnrolmentChanges {
    /// How long from now. Given instead of a moment because that is how access
    /// is sold — ninety days, a year — and a date somebody types is a date in
    /// their own timezone.
    #[serde(default)]
    pub days: Option<i32>,
    /// No end at all. Said explicitly, because a missing `days` means "leave it
    /// alone" rather than "for ever".
    #[serde(default)]
    pub forever: Option<bool>,
}

/// Somebody a site teaches.
#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Student {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub state: StudentState,
    /// When they were last actually here. Null is never.
    pub last_seen_at: Option<DateTime<Utc>>,
    /// How many courses they are on. What somebody looks at before suspending.
    pub courses: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StudentChanges {
    pub name: Option<Title>,
    /// Stopping somebody, or letting them back in.
    pub suspended: Option<bool>,
}

/// Somebody on a course. Not [`Enrolled`], which carries the token that put
/// them there and is shown once.
#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct OnCourse {
    pub student_id: Uuid,
    pub email: String,
    pub name: String,
    pub enrolled_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct StudentCredentials {
    pub email: Email,
    pub password: Secret<String>,
}

async fn list(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
    HttpQuery(query): HttpQuery<Query>,
) -> Result<Json<Page<Course>>> {
    let mut conn = state.db.begin().await?;

    let rows: Vec<Course> = sqlx::query_as(
        "select id, slug, title, summary, state, created_at
           from courses
          where deleted_at is null
            and ($1::timestamptz is null or created_at < $1)
          order by created_at desc, id desc limit $2",
    )
    .bind(cursor(query.after.as_deref()))
    .bind(query.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&query, rows, |course| {
        course.created_at.to_rfc3339()
    })))
}

fn cursor(after: Option<&str>) -> Option<DateTime<Utc>> {
    after.and_then(|value| DateTime::parse_from_rfc3339(value).ok().map(Into::into))
}

/// In the site's own words where it has written any, the same two questions
/// [`crate::people`] and [`crate::shop`] ask before pressing a letter.
async fn site_language(conn: &mut Tx) -> Result<String> {
    let found: Option<(String,)> =
        sqlx::query_as("select code from languages where is_default limit 1")
            .fetch_optional(conn.conn())
            .await?;

    Ok(found.map_or_else(|| "en".to_owned(), |(code,)| code))
}

async fn site_name(conn: &mut Tx) -> Result<String> {
    let found: Option<(String,)> = sqlx::query_as("select name from site_settings")
        .fetch_optional(conn.conn())
        .await?;

    Ok(found.map_or_else(String::new, |(name,)| name))
}

/// What a student is told once they are put on a course. Whoever enrolled
/// them still gets the password back in the response — the panel is signed
/// in as them, not as the student — so this letter is a courtesy rather than
/// the only way in.
async fn told_the_student(
    conn: &mut Tx,
    state: &AppState,
    _caller: &Caller,
    email: &str,
    name: &str,
) -> Result<()> {
    let language = site_language(conn).await?;
    let site = site_name(conn).await?;

    let (subject, letter) = crate::mail::letters::press(
        conn,
        "student.invited",
        &language,
        &[
            ("name", name.to_owned()),
            ("site", site),
            ("link", state.address.link("/")),
        ],
    )
    .await?;

    crate::mail::post(
        conn,
        &crate::mail::Outgoing {
            to: email,
            subject: &subject,
            body: &letter,
            purpose: crate::mail::Purpose::Transactional,
            campaign_id: None,
            subscriber_id: None,
            unsubscribe: None,
        },
    )
    .await?;

    Ok(())
}

async fn create(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<NewCourse>,
) -> Result<Audited<(StatusCode, Json<Course>)>> {
    let mut conn = state.db.begin().await?;

    let course: Course = sqlx::query_as(
        "insert into courses (slug, title, summary) values ($1, $2, $3)
         returning id, slug, title, summary, state, created_at",
    )
    .bind(body.slug.as_str())
    .bind(body.title.as_str())
    .bind(body.summary.as_deref())
    .fetch_one(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            Some(code) if code == "23505" => {
                AppError::Conflict(say::COURSE_ALREADY_ANSWERS_NAME.into())
            }
            _ => AppError::Database(error),
        }
    })?;

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "made a course",
        None,
        Some(&course),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(course))))
}

/// Puts somebody on a course, making them a student here if they were not one.
/// Everybody a site teaches, whatever they are on.
///
/// The list a panel shows when the question is about a person rather than about
/// a course: what they are called, whether they are still allowed in, and how
/// many courses they are on.
async fn students(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
    axum::extract::Query(page): axum::extract::Query<Query>,
) -> Result<Json<Page<Student>>> {
    let mut conn = state.db.begin().await?;

    let rows: Vec<Student> = sqlx::query_as(
        "select s.id, s.email, s.name, s.state, s.last_seen_at, s.created_at,
                (select count(*) from enrolments e where e.student_id = s.id) as courses
           from students s
          where s.deleted_at is null
            and ($1::timestamptz is null or s.created_at < $1)
          order by s.created_at desc
          limit $2",
    )
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&page, rows, |student| {
        student.created_at.to_rfc3339()
    })))
}

/// Letting somebody back in, or stopping them.
///
/// Suspending is not deleting: what they finished stays finished, and letting
/// them back in is one call rather than an enrolment written again.
async fn change_student(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    Json(wanted): Json<StudentChanges>,
) -> Result<Audited<Json<Student>>> {
    let mut conn = state.db.begin().await?;

    let changed: Option<Student> = sqlx::query_as(
        "update students s
            set name = coalesce($2, s.name),
                state = case
                    when $3 is null then s.state
                    when $3 then 'suspended'::student_state
                    else 'active'::student_state
                end
          where s.id = $1 and s.deleted_at is null
         returning s.id, s.email, s.name, s.state, s.last_seen_at, s.created_at,
                   (select count(*) from enrolments e where e.student_id = s.id) as courses",
    )
    .bind(id)
    .bind(wanted.name.as_ref().map(Title::as_str))
    .bind(wanted.suspended)
    .fetch_optional(conn.conn())
    .await?;

    let changed = changed.ok_or(AppError::NotFound("student"))?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "changed a student",
        "student",
        Some(&changed.id.to_string()),
        &serde_json::json!({ "state": changed.state }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(changed)))
}

const ENROLMENT_COLUMNS: &str = "e.id, e.course_id, c.title as course, e.ends_at,
     case when c.state <> 'open' then 'waiting'
          when e.ends_at is not null and e.ends_at <= now() then 'ended'
          else 'open' end as state,
     e.created_at";

/// Who is on a course. What a panel shows beside it, and the answer to "did
/// that person's enrolment go through".
async fn enrolled(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
    Path(course_id): Path<Uuid>,
    axum::extract::Query(page): axum::extract::Query<Query>,
) -> Result<Json<Page<OnCourse>>> {
    let mut conn = state.db.begin().await?;

    let rows: Vec<OnCourse> = sqlx::query_as(
        "select s.id as student_id, s.email, s.name, e.created_at as enrolled_at
           from enrolments e join students s on s.id = e.student_id
          where e.course_id = $1 and s.deleted_at is null
            and ($2::timestamptz is null or e.created_at < $2)
          order by e.created_at desc
          limit $3",
    )
    .bind(course_id)
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&page, rows, |on| {
        on.enrolled_at.to_rfc3339()
    })))
}

async fn enrol(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(course_id): Path<Uuid>,
    Json(body): Json<Enrolling>,
) -> Result<Audited<(StatusCode, Json<Enrolled>)>> {
    let mut conn = state.db.begin().await?;

    let secret = token::generate();

    let student: (Uuid,) = sqlx::query_as(
        "insert into students (email, name, state, password_hash)
         values ($1, $2, 'active', $3)
         on conflict (email) do update set name = excluded.name
         returning id",
    )
    .bind(body.email.as_str())
    .bind(body.name.as_str())
    .bind(password::hash(&secret)?)
    .fetch_one(conn.conn())
    .await?;

    sqlx::query(
        "insert into enrolments (student_id, course_id, ends_at)
         values ($1, $2,
                 case when $3::int is null then null
                      else now() + make_interval(days => $3) end)
         on conflict (student_id, course_id) do update
             set ends_at = excluded.ends_at",
    )
    .bind(student.0)
    .bind(course_id)
    .bind(body.days)
    .execute(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            Some(code) if code == "23503" => AppError::NotFound("course"),
            _ => AppError::Database(error),
        }
    })?;

    told_the_student(
        &mut conn,
        &state,
        &caller,
        body.email.as_str(),
        body.name.as_str(),
    )
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "enrolled",
        "student",
        Some(&student.0.to_string()),
        &serde_json::json!({ "course_id": course_id }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        (
            StatusCode::CREATED,
            Json(Enrolled {
                student_id: student.0,
                token: Shown::new(secret),
            }),
        ),
    ))
}

/// A course as whoever is writing it sees it: every module and lesson, open or
/// not, and no student's progress in it.
async fn whole_course(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Json<Curriculum>> {
    let mut conn = state.db.begin().await?;

    let course: Course = sqlx::query_as(
        "select id, slug, title, summary, state, created_at
           from courses where id = $1 and deleted_at is null",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?
    .ok_or(AppError::NotFound("course"))?;

    let rows = sqlx::query(
        "select m.id as module_id, m.title as module_title, m.position as module_position,
                l.id as lesson_id, l.title as lesson_title, l.position as lesson_position,
                l.video_id
           from modules m
           left join lessons l on l.module_id = m.id
          where m.course_id = $1
          order by m.position, l.position",
    )
    .bind(id)
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    let mut modules: Vec<Module> = Vec::new();

    for row in rows {
        let module_id: Uuid = row.get("module_id");

        if modules.last().map(|module| module.id) != Some(module_id) {
            modules.push(Module {
                id: module_id,
                title: row.get("module_title"),
                position: row.get("module_position"),
                lessons: Vec::new(),
            });
        }

        let lesson_id: Option<Uuid> = row.get("lesson_id");

        if let (Some(lesson_id), Some(module)) = (lesson_id, modules.last_mut()) {
            module.lessons.push(Lesson {
                id: lesson_id,
                title: row.get("lesson_title"),
                position: row.get("lesson_position"),
                video_id: row.get("video_id"),
                done: false,
            });
        }
    }

    Ok(Json(Curriculum { course, modules }))
}

async fn change_course(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    Json(wanted): Json<CourseChanges>,
) -> Result<Audited<Json<Course>>> {
    if wanted
        .state
        .as_deref()
        .is_some_and(|state| !matches!(state, "draft" | "open" | "closed"))
    {
        return Err(AppError::Invalid(
            say::THAT_IS_NOT_A_STATE_A_COURSE_IS_IN.into(),
        ));
    }

    let mut conn = state.db.begin().await?;

    let after: Option<Course> = sqlx::query_as(
        "update courses
            set title = coalesce($2, title),
                summary = coalesce($3, summary),
                state = coalesce($4::course_state, state)
          where id = $1 and deleted_at is null
         returning id, slug, title, summary, state, created_at",
    )
    .bind(id)
    .bind(wanted.title.as_ref().map(Title::as_str))
    .bind(wanted.summary.as_deref())
    .bind(wanted.state.as_deref())
    .fetch_optional(conn.conn())
    .await?;

    let after = after.ok_or(AppError::NotFound("course"))?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "changed a course",
        "course",
        Some(&after.id.to_string()),
        &serde_json::json!({ "state": after.state }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(after)))
}

/// A module, at the end unless somebody says where.
async fn add_a_module(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(course_id): Path<Uuid>,
    Json(body): Json<NewModule>,
) -> Result<Audited<(StatusCode, Json<Module>)>> {
    let mut conn = state.db.begin().await?;

    let made: (Uuid, String, i32) = sqlx::query_as(
        "insert into modules (course_id, title, position)
         values ($1, $2,
                 coalesce($3, (select coalesce(max(position) + 1, 0) from modules
                                where course_id = $1)))
         returning id, title, position",
    )
    .bind(course_id)
    .bind(body.title.as_str())
    .bind(body.position)
    .fetch_one(conn.conn())
    .await
    .map_err(where_it_sits)?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "added a module",
        "module",
        Some(&made.0.to_string()),
        &serde_json::json!({ "course_id": course_id }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        (
            StatusCode::CREATED,
            Json(Module {
                id: made.0,
                title: made.1,
                position: made.2,
                lessons: Vec::new(),
            }),
        ),
    ))
}

async fn change_module(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    Json(wanted): Json<ModuleChanges>,
) -> Result<Audited<Json<Module>>> {
    let mut conn = state.db.begin().await?;

    let after: Option<(Uuid, String, i32)> = sqlx::query_as(
        "update modules
            set title = coalesce($2, title), position = coalesce($3, position)
          where id = $1
         returning id, title, position",
    )
    .bind(id)
    .bind(wanted.title.as_ref().map(Title::as_str))
    .bind(wanted.position)
    .fetch_optional(conn.conn())
    .await
    .map_err(where_it_sits)?;

    let after = after.ok_or(AppError::NotFound("module"))?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "changed a module",
        "module",
        Some(&id.to_string()),
        &serde_json::json!({ "position": after.2 }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        Json(Module {
            id: after.0,
            title: after.1,
            position: after.2,
            lessons: Vec::new(),
        }),
    ))
}

/// A module, and the lessons in it.
///
/// What a student finished stays: progress is about a lesson that existed,
/// and the row goes with the lesson rather than being kept pointing at
/// nothing.
async fn remove_module(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.begin().await?;

    let gone = sqlx::query("delete from modules where id = $1")
        .bind(id)
        .execute(conn.conn())
        .await?
        .rows_affected();

    if gone == 0 {
        return Err(AppError::NotFound("module"));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "took a module away",
        "module",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

async fn add_a_lesson(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(module_id): Path<Uuid>,
    Json(body): Json<NewLesson>,
) -> Result<Audited<(StatusCode, Json<Lesson>)>> {
    let mut conn = state.db.begin().await?;

    let made: (Uuid, String, i32, Option<Uuid>) = sqlx::query_as(
        "insert into lessons (module_id, title, body, position, video_id)
         values ($1, $2, coalesce($3, ''),
                 coalesce($4, (select coalesce(max(position) + 1, 0) from lessons
                                where module_id = $1)),
                 $5)
         returning id, title, position, video_id",
    )
    .bind(module_id)
    .bind(body.title.as_str())
    .bind(body.body.as_deref())
    .bind(body.position)
    .bind(body.video_id)
    .fetch_one(conn.conn())
    .await
    .map_err(where_it_sits)?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "added a lesson",
        "lesson",
        Some(&made.0.to_string()),
        &serde_json::json!({ "module_id": module_id }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        (
            StatusCode::CREATED,
            Json(Lesson {
                id: made.0,
                title: made.1,
                position: made.2,
                video_id: made.3,
                done: false,
            }),
        ),
    ))
}

async fn change_lesson(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    Json(wanted): Json<LessonChanges>,
) -> Result<Audited<Json<Lesson>>> {
    let mut conn = state.db.begin().await?;

    let after: Option<(Uuid, String, i32, Option<Uuid>)> = sqlx::query_as(
        "update lessons
            set title = coalesce($2, title),
                position = coalesce($3, position),
                body = coalesce($4, body),
                video_id = coalesce($5, video_id)
          where id = $1
         returning id, title, position, video_id",
    )
    .bind(id)
    .bind(wanted.title.as_ref().map(Title::as_str))
    .bind(wanted.position)
    .bind(wanted.body.as_deref())
    .bind(wanted.video_id)
    .fetch_optional(conn.conn())
    .await
    .map_err(where_it_sits)?;

    let after = after.ok_or(AppError::NotFound("lesson"))?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "changed a lesson",
        "lesson",
        Some(&id.to_string()),
        &serde_json::json!({ "position": after.2 }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        Json(Lesson {
            id: after.0,
            title: after.1,
            position: after.2,
            video_id: after.3,
            done: false,
        }),
    ))
}

async fn remove_lesson(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.begin().await?;

    let gone = sqlx::query("delete from lessons where id = $1")
        .bind(id)
        .execute(conn.conn())
        .await?
        .rows_affected();

    if gone == 0 {
        return Err(AppError::NotFound("lesson"));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "took a lesson away",
        "lesson",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

/// Two things in one place is what the unique index refuses, and what a
/// person means is "put it there and move the rest" — which nothing here can
/// guess, so it is said rather than done.
fn where_it_sits(error: sqlx::Error) -> AppError {
    match error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
    {
        Some(code) if code == "23505" => {
            AppError::Conflict(say::SOMETHING_IS_ALREADY_IN_THAT_PLACE.into())
        }
        Some(code) if code == "23503" => AppError::NotFound("course"),
        other => {
            let _ = other;
            AppError::Database(error)
        }
    }
}

/// What one person is on.
async fn enrolments_of(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Enrolment>>> {
    let mut conn = state.db.begin().await?;

    let rows: Vec<Enrolment> = sqlx::query_as(&format!(
        "select {ENROLMENT_COLUMNS}
           from enrolments e join courses c on c.id = e.course_id
          where e.student_id = $1
          order by e.created_at desc"
    ))
    .bind(id)
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(rows))
}

/// Giving somebody longer, or taking the end off altogether.
async fn change_enrolment(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    Json(wanted): Json<EnrolmentChanges>,
) -> Result<Audited<Json<Enrolment>>> {
    if wanted.days.is_some_and(|days| days <= 0) {
        return Err(AppError::Invalid(say::THAT_IS_NOT_A_LENGTH_OF_TIME.into()));
    }

    let mut conn = state.db.begin().await?;

    let changed: Option<Enrolment> = sqlx::query_as(&format!(
        "with changed as (
             update enrolments
                set ends_at = case
                    when $2 then null
                    when $3::int is null then ends_at
                    else now() + make_interval(days => $3)
                end
              where id = $1
             returning id, course_id, ends_at, created_at
         )
         select {ENROLMENT_COLUMNS}
           from changed e join courses c on c.id = e.course_id"
    ))
    .bind(id)
    .bind(wanted.forever.unwrap_or(false))
    .bind(wanted.days)
    .fetch_optional(conn.conn())
    .await?;

    let changed = changed.ok_or(AppError::NotFound("enrolment"))?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "changed an enrolment",
        "enrolment",
        Some(&changed.id.to_string()),
        &serde_json::json!({ "ends_at": changed.ends_at }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(changed)))
}

/// Taking access back.
///
/// The row goes: what they finished stays, because progress is about a lesson
/// rather than about being allowed to watch it, and somebody put back on the
/// course should not start again from nothing.
async fn revoke(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.begin().await?;

    let gone = sqlx::query("delete from enrolments where id = $1")
        .bind(id)
        .execute(conn.conn())
        .await?
        .rows_affected();

    if gone == 0 {
        return Err(AppError::NotFound("enrolment"));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "took an enrolment back",
        "enrolment",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

/// A student's own way in, at the site's front. It sets the student's cookie
/// and nothing else: this token opens nothing in the panel.
async fn sign_in(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Json(credentials): Json<StudentCredentials>,
) -> Result<Audited<Response>> {
    let mut conn = state.db.begin().await?;

    let found: Option<(Uuid, Option<String>)> = sqlx::query_as(
        "select id, password_hash from students
          where email = $1 and state = 'active' and deleted_at is null",
    )
    .bind(credentials.email.as_str())
    .fetch_optional(conn.conn())
    .await?;

    let no = AppError::Invalid(say::NOT_SIGN_WE_KNOW.into());

    let Some((id, Some(stored))) = found else {
        password::waste_the_same_time(credentials.password.expose());
        return Err(no);
    };

    if !password::verify(credentials.password.expose(), &stored) {
        return Err(no);
    }

    let secret = token::generate();
    let expires_at = state.clock.now() + Duration::days(SESSION_DAYS);

    sqlx::query(
        "update student_sessions set revoked_at = now()
          where student_id = $1 and revoked_at is null",
    )
    .bind(id)
    .execute(conn.conn())
    .await?;

    sqlx::query("update students set last_seen_at = now() where id = $1")
        .bind(id)
        .execute(conn.conn())
        .await?;

    sqlx::query(
        "insert into student_sessions (student_id, token_hash, expires_at)
         values ($1, $2, $3)",
    )
    .bind(id)
    .bind(&token::hash(&secret)[..])
    .bind(expires_at)
    .execute(conn.conn())
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::system(caller.request_id),
        "signed in",
        "student",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    let mut response = StatusCode::NO_CONTENT.into_response();

    response.headers_mut().insert(
        SET_COOKIE,
        format!(
            "{STUDENT_COOKIE}={secret}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age={}",
            SESSION_DAYS * 24 * 60 * 60
        )
        .parse()
        .map_err(|_| AppError::Bug("a cookie this shape is always a header value"))?,
    );

    Ok(Audited::new(receipt, response))
}

/// What this student is on. Not what the site teaches: only what they were put
/// on, which is the whole difference between this and the panel's list.
async fn mine(
    Injected(state): Injected<AppState>,
    caller: Caller,
    axum::extract::Query(page): axum::extract::Query<Query>,
) -> Result<Json<Page<Course>>> {
    let student = caller.require_student()?;
    let mut conn = state.db.begin().await?;

    let rows: Vec<Course> = sqlx::query_as(
        "select c.id, c.slug, c.title, c.summary, c.state, c.created_at
           from enrolments e join courses c on c.id = e.course_id
          where e.student_id = $1 and c.deleted_at is null and c.state = 'open'
            and (e.ends_at is null or e.ends_at > now())
            and ($2::timestamptz is null or c.created_at < $2)
          order by c.created_at desc
          limit $3",
    )
    .bind(student.student_id)
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&page, rows, |course| {
        course.created_at.to_rfc3339()
    })))
}

/// Whoever is watching, as their own screen shows them.
#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Learner {
    pub id: Uuid,
    pub email: String,
    pub name: String,
}

async fn student_me(Injected(state): Injected<AppState>, caller: Caller) -> Result<Json<Learner>> {
    let student = caller.require_student()?;
    let mut conn = state.db.begin().await?;

    let found: Option<Learner> =
        sqlx::query_as("select id, email, name from students where id = $1")
            .bind(student.student_id)
            .fetch_optional(conn.conn())
            .await?;

    conn.commit().await?;

    found.map(Json).ok_or(AppError::Unauthenticated)
}

/// One lesson, as somebody on the course reads it.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Watching {
    pub id: Uuid,
    pub title: String,
    /// What the lesson says. Kept when access ends: it is the video that
    /// stops, not the notes.
    pub body: String,
    pub course_id: Uuid,
    pub course: String,
    /// Which of how many, over the whole course.
    pub position: i64,
    pub total: i64,
    pub previous: Option<Uuid>,
    pub next: Option<Uuid>,
    /// What to play, where there is one and this person may watch it.
    pub video_id: Option<Uuid>,
    pub done: bool,
}

/// Whoever a student's cookie belongs to, and nobody if the session has been
/// revoked, has run out, or the student is no longer on the site.
///
/// Handed to the kernel rather than read by it: `student_sessions` is this
/// module's table, and a kernel that read it would be a kernel that has to know
/// there are students at all.
#[must_use]
pub fn a_student<'a>(db: &'a Db, token: &'a str) -> Answers<'a, Option<SignedInStudent>> {
    Box::pin(whose_session(db, token))
}

async fn whose_session(db: &Db, token: &str) -> Result<Option<SignedInStudent>> {
    let hash = token::hash(token);
    let mut conn = db.begin().await?;

    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "update student_sessions s
            set last_seen_at = now()
           from students t
          where s.token_hash = $1
            and s.student_id = t.id
            and s.revoked_at is null
            and s.expires_at > now()
            and t.state = 'active'
            and t.deleted_at is null
         returning s.id, t.id",
    )
    .bind(&hash[..])
    .fetch_optional(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(row.map(|(session_id, student_id)| SignedInStudent {
        student_id,
        session_id,
    }))
}

/// Signing out, which is the student's own session and nothing else.
async fn sign_out(
    Injected(state): Injected<AppState>,
    caller: Caller,
) -> Result<Audited<Response>> {
    let student = caller.require_student()?;
    let mut conn = state.db.begin().await?;

    sqlx::query(
        "update student_sessions set revoked_at = now()
          where student_id = $1 and revoked_at is null",
    )
    .bind(student.student_id)
    .execute(conn.conn())
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "a student signed out",
        "student",
        Some(&student.student_id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, {
        let mut response = StatusCode::NO_CONTENT.into_response();

        response.headers_mut().insert(
            SET_COOKIE,
            format!("{STUDENT_COOKIE}=; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=0")
                .parse()
                .map_err(|_| AppError::Bug("a cookie this shape is always a header value"))?,
        );

        response
    }))
}

/// One lesson: what it says, what plays, and what is either side of it.
///
/// Refused where they are not on the course or their access has ended — the
/// same rule the listing uses, asked again here because a lesson's id is a
/// thing somebody can type.
async fn watch(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
) -> Result<Json<Watching>> {
    let student = caller.require_student()?;
    let mut conn = state.db.begin().await?;

    let found: Option<(Uuid, String, String, Option<Uuid>, Uuid, String)> = sqlx::query_as(
        "select l.id, l.title, l.body, l.video_id, c.id as course_id, c.title as course
           from lessons l
           join modules m on m.id = l.module_id
           join courses c on c.id = m.course_id
           join enrolments e on e.course_id = c.id and e.student_id = $2
          where l.id = $1
            and c.state = 'open' and c.deleted_at is null
            and (e.ends_at is null or e.ends_at > now())",
    )
    .bind(id)
    .bind(student.student_id)
    .fetch_optional(conn.conn())
    .await?;

    let Some((id, title, body, video_id, course_id, course)) = found else {
        return Err(AppError::NotFound("lesson"));
    };

    // Where it sits in the whole course, and what is either side of it: a
    // player without "next" is a player somebody closes after one lesson.
    let order: Vec<(Uuid,)> = sqlx::query_as(
        "select l.id from lessons l
           join modules m on m.id = l.module_id
          where m.course_id = $1
          order by m.position, l.position",
    )
    .bind(course_id)
    .fetch_all(conn.conn())
    .await?;

    let done: Option<(Uuid,)> = sqlx::query_as(
        "select lesson_id from lesson_progress where lesson_id = $1 and student_id = $2",
    )
    .bind(id)
    .bind(student.student_id)
    .fetch_optional(conn.conn())
    .await?;

    conn.commit().await?;

    let at = order.iter().position(|(one,)| *one == id).unwrap_or(0);

    Ok(Json(Watching {
        id,
        title,
        body,
        course_id,
        course,
        position: i64::try_from(at + 1).unwrap_or(1),
        total: i64::try_from(order.len()).unwrap_or(0),
        previous: at
            .checked_sub(1)
            .and_then(|which| order.get(which))
            .map(|(one,)| *one),
        next: order.get(at + 1).map(|(one,)| *one),
        video_id,
        done: done.is_some(),
    }))
}

/// The video a lesson plays, for somebody who may watch it.
///
/// Served here rather than from `/uploads`, which is public: a picture on a
/// page is meant to be seen by anybody and a course's video is not, and an
/// address that cannot be guessed is not the same as one that cannot be
/// shared.
async fn play(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
) -> Result<Response> {
    let student = caller.require_student()?;
    let mut conn = state.db.begin().await?;

    let found: Option<(String, String)> = sqlx::query_as(
        "select m.location, m.mime
           from videos v
           join media m on m.id = v.media_id
          where v.id = $1 and v.state = 'ready' and m.deleted_at is null
            and exists (
                select 1 from lessons l
                  join modules mo on mo.id = l.module_id
                  join enrolments e on e.course_id = mo.course_id
                 where l.video_id = v.id and e.student_id = $2
                   and (e.ends_at is null or e.ends_at > now())
            )",
    )
    .bind(id)
    .bind(student.student_id)
    .fetch_optional(conn.conn())
    .await?;

    conn.commit().await?;

    let Some((location, mime)) = found else {
        return Err(AppError::NotFound("video"));
    };

    // The kind this machine decided when the bytes arrived, not the string in
    // the row: a lesson pointing at something that is not a video would
    // otherwise be a page running on the site's own address, in front of
    // whoever is on the course.
    let allowed = crate::kernel::storage::allowed_for(&mime)
        .filter(|allowed| allowed.mime.starts_with("video/"))
        .ok_or(AppError::NotFound("video"))?;

    let bytes = state.store.get(&location).await?;

    let mut response = bytes.into_response();
    let headers = response.headers_mut();

    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static(allowed.mime),
    );

    // Never guessed from the bytes by whoever is reading: the type above is
    // the one this machine decided, and nosniff is what makes it stick.
    headers.insert(
        "x-content-type-options",
        axum::http::HeaderValue::from_static("nosniff"),
    );

    // Never by anything in between: what a student may watch is decided per
    // request, and a cached copy is one that outlives their access.
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("private, no-store"),
    );

    Ok(response)
}

/// A course, its modules and lessons, and what this student has finished — in
/// three queries rather than one per lesson.
async fn curriculum(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Path(course_id): Path<Uuid>,
) -> Result<Json<Curriculum>> {
    let student = caller.require_student()?;
    let mut conn = state.db.begin().await?;

    let on_it: Option<(Uuid,)> = sqlx::query_as(
        "select id from enrolments
              where student_id = $1 and course_id = $2
                and (ends_at is null or ends_at > now())",
    )
    .bind(student.student_id)
    .bind(course_id)
    .fetch_optional(conn.conn())
    .await?;

    if on_it.is_none() {
        return Err(AppError::NotFound("course"));
    }

    let course: Course = sqlx::query_as(
        "select id, slug, title, summary, state, created_at
           from courses where id = $1 and deleted_at is null and state = 'open'",
    )
    .bind(course_id)
    .fetch_optional(conn.conn())
    .await?
    .ok_or(AppError::NotFound("course"))?;

    let rows = sqlx::query(
        "select m.id as module_id, m.title as module_title, m.position as module_position,
                l.id as lesson_id, l.title as lesson_title, l.position as lesson_position,
                l.video_id, p.student_id is not null as done
           from modules m
           left join lessons l on l.module_id = m.id
           left join lesson_progress p on p.lesson_id = l.id and p.student_id = $2
          where m.course_id = $1
          order by m.position, l.position",
    )
    .bind(course_id)
    .bind(student.student_id)
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    let mut modules: Vec<Module> = Vec::new();

    for row in rows {
        let module_id: Uuid = row.get("module_id");

        if modules.last().map(|module| module.id) != Some(module_id) {
            modules.push(Module {
                id: module_id,
                title: row.get("module_title"),
                position: row.get("module_position"),
                lessons: Vec::new(),
            });
        }

        if let Some(lesson_id) = row.get::<Option<Uuid>, _>("lesson_id")
            && let Some(module) = modules.last_mut()
        {
            module.lessons.push(Lesson {
                id: lesson_id,
                title: row.get("lesson_title"),
                position: row.get("lesson_position"),
                video_id: row.get("video_id"),
                done: row.get("done"),
            });
        }
    }

    Ok(Json(Curriculum { course, modules }))
}

/// Finishing a lesson. Only for a lesson on a course this student is on, and
/// only ever for themselves.
async fn mark_done(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Path(lesson_id): Path<Uuid>,
) -> Result<Audited<StatusCode>> {
    let student = caller.require_student()?;
    let mut conn = state.db.begin().await?;

    let allowed: Option<(Uuid,)> = sqlx::query_as(
        "select l.id
           from lessons l
           join modules m on m.id = l.module_id
           join enrolments e on e.course_id = m.course_id and e.student_id = $2
          where l.id = $1",
    )
    .bind(lesson_id)
    .bind(student.student_id)
    .fetch_optional(conn.conn())
    .await?;

    if allowed.is_none() {
        return Err(AppError::NotFound("lesson"));
    }

    sqlx::query(
        "insert into lesson_progress (student_id, lesson_id)
         values ($1, $2) on conflict do nothing",
    )
    .bind(student.student_id)
    .bind(lesson_id)
    .execute(conn.conn())
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::system(caller.request_id),
        "finished a lesson",
        "lesson",
        Some(&lesson_id.to_string()),
        &serde_json::json!({ "student_id": student.student_id }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

/// Used by the panel to build a course; kept here rather than in a handler of
/// its own because a module without a course is not a thing.
pub async fn add_module(
    conn: &mut Tx,
    course_id: Uuid,
    title: &str,
    position: i32,
) -> Result<Uuid> {
    let row = sqlx::query(
        "insert into modules (course_id, title, position)
         values ($1, $2, $3) returning id",
    )
    .bind(course_id)
    .bind(title)
    .bind(position)
    .fetch_one(conn.conn())
    .await?;

    Ok(row.get("id"))
}

pub async fn add_lesson(
    conn: &mut Tx,
    module_id: Uuid,
    title: &str,
    position: i32,
) -> Result<Uuid> {
    let row = sqlx::query(
        "insert into lessons (module_id, title, position)
         values ($1, $2, $3) returning id",
    )
    .bind(module_id)
    .bind(title)
    .bind(position)
    .fetch_one(conn.conn())
    .await?;

    Ok(row.get("id"))
}
