//! Reading and writing courses, and what somebody has worked through.
//!
//! Two rules from elsewhere in this crate are enforced here rather than
//! repeated: a reorder is the same things rearranged, and a student opens a
//! lesson only if all three of the things [`crate::student::may_open`] asks
//! about are true.

use chrono::{DateTime, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::page::{Page, Query};
use mavi_core::say::Say;
use mavi_core::slug::Slug;
use mavi_db::{Tx, Walk};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::postgres::PgRow;
use uuid::Uuid;

use crate::sequence::in_this_order;
use crate::student::{Standing, may_open};
use crate::{BY_RECENT, State};

pub const THERE_IS_NO_COURSE_LIKE_THAT: &str = "there_is_no_course_like_that";
pub const THERE_IS_NO_LESSON_LIKE_THAT: &str = "there_is_no_lesson_like_that";
pub const THERE_IS_NO_MODULE_LIKE_THAT: &str = "there_is_no_module_like_that";
pub const SOMETHING_ELSE_IS_TAUGHT_AT_THAT_ADDRESS: &str =
    "something_else_is_taught_at_that_address";
pub const NOBODY_HERE_IS_LEARNING_UNDER_THAT: &str = "nobody_here_is_learning_under_that";
pub const SOMEBODY_ALREADY_LEARNS_HERE_UNDER_THAT_ADDRESS: &str =
    "somebody_already_learns_here_under_that_address";

/// One course.
#[derive(Clone, Debug, Serialize)]
pub struct Course {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub about: Option<String>,
    pub state: State,
    /// Empty in a listing. What a listing is for is choosing a course, and
    /// carrying every lesson of every one of them is a page that grows with
    /// the site rather than with the screen.
    pub modules: Vec<Module>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Module {
    pub id: Uuid,
    pub title: String,
    pub place: i32,
    pub lessons: Vec<Lesson>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Lesson {
    pub id: Uuid,
    pub module_id: Uuid,
    pub title: String,
    pub body: String,
    pub place: i32,
}

fn a_state(said: &str) -> State {
    match said {
        "open" => State::Open,
        "closed" => State::Closed,
        _ => State::Draft,
    }
}

fn a_course(row: &PgRow) -> Result<Course> {
    let state: String = row.try_get("state").map_err(Error::internal)?;

    Ok(Course {
        id: row.try_get("id").map_err(Error::internal)?,
        slug: row.try_get("slug").map_err(Error::internal)?,
        title: row.try_get("title").map_err(Error::internal)?,
        about: row.try_get("about").map_err(Error::internal)?,
        state: a_state(&state),
        modules: Vec::new(),
        created_at: row.try_get("created_at").map_err(Error::internal)?,
    })
}

fn a_lesson(row: &PgRow) -> Result<Lesson> {
    Ok(Lesson {
        id: row.try_get("id").map_err(Error::internal)?,
        module_id: row.try_get("module_id").map_err(Error::internal)?,
        title: row.try_get("title").map_err(Error::internal)?,
        body: row.try_get("body").map_err(Error::internal)?,
        place: row.try_get("place").map_err(Error::internal)?,
    })
}

/// Every course, newest first.
pub async fn list(tx: &mut Tx, state: Option<&str>, query: &Query) -> Result<Page<Course>> {
    let walk = Walk::new(BY_RECENT, query.after(BY_RECENT)?);
    let mut wheres = vec!["deleted_at is null".to_owned()];
    let mut binds: Vec<String> = Vec::new();

    if let Some(state) = state {
        binds.push(state.to_owned());
        wheres.push(format!("state = ${}", binds.len()));
    }

    let cursor = walk.after(binds.len() + 1);
    if let Some((sql, _)) = &cursor {
        wheres.push(sql.clone());
    }

    let sql = format!(
        "select id, slug, title, about, state, created_at from courses
          where {} order by {} limit {}",
        wheres.join(" and "),
        walk.order(),
        query.fetch(),
    );

    let mut asking = sqlx::query(&sql);

    for bind in binds {
        asking = asking.bind(bind);
    }

    if let Some((_, values)) = cursor {
        for value in values {
            asking = asking.bind(value);
        }
    }

    let courses = asking
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?
        .iter()
        .map(a_course)
        .collect::<Result<Vec<_>>>()?;

    Page::build(query, BY_RECENT, courses, |course| {
        vec![course.created_at.to_rfc3339(), course.id.to_string()]
    })
}

/// What starting one asks for.
///
/// Serialised as well as read, so the test beside the description can hold
/// what it says it takes against what it takes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewCourse {
    pub slug: String,
    pub title: String,
    pub about: Option<String>,
}

/// Starts one.
pub async fn make(tx: &mut Tx, new: &NewCourse) -> Result<Course> {
    let slug = Slug::parse(&new.slug)?;
    let id = Uuid::now_v7();

    sqlx::query("insert into courses (id, slug, title, about) values ($1, $2, $3, $4)")
        .bind(id)
        .bind(slug.as_str())
        .bind(new.title.trim())
        .bind(new.about.as_deref())
        .execute(tx.conn())
        .await
        .map_err(|cause| match &cause {
            sqlx::Error::Database(db) if db.constraint() == Some("courses_address") => {
                Error::conflict(Say::of(SOMETHING_ELSE_IS_TAUGHT_AT_THAT_ADDRESS))
            }
            _ => Error::internal(cause),
        })?;

    read(tx, id).await
}

/// One course, its modules and its lessons, in order.
pub async fn read(tx: &mut Tx, id: Uuid) -> Result<Course> {
    let row = sqlx::query(
        "select id, slug, title, about, state, created_at from courses
          where id = $1 and deleted_at is null",
    )
    .bind(id)
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?
    .ok_or_else(|| Error::not_found(Say::of(THERE_IS_NO_COURSE_LIKE_THAT)))?;

    let mut course = a_course(&row)?;
    course.modules = modules(tx, id).await?;

    Ok(course)
}

async fn modules(tx: &mut Tx, course: Uuid) -> Result<Vec<Module>> {
    let rows =
        sqlx::query("select id, title, place from modules where course_id = $1 order by place")
            .bind(course)
            .fetch_all(tx.conn())
            .await
            .map_err(Error::internal)?;

    let mut modules = Vec::with_capacity(rows.len());

    for row in &rows {
        let id: Uuid = row.try_get("id").map_err(Error::internal)?;

        modules.push(Module {
            id,
            title: row.try_get("title").map_err(Error::internal)?,
            place: row.try_get("place").map_err(Error::internal)?,
            lessons: lessons(tx, id).await?,
        });
    }

    Ok(modules)
}

async fn lessons(tx: &mut Tx, module: Uuid) -> Result<Vec<Lesson>> {
    let rows = sqlx::query(
        "select id, module_id, title, body, place from lessons where module_id = $1 order by place",
    )
    .bind(module)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter().map(a_lesson).collect()
}

/// What may be changed about a course.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CourseChanges {
    pub title: Option<String>,
    pub about: Option<String>,
    /// `draft`, `open` or `closed`.
    pub state: Option<String>,
}

/// Renames one, or opens or closes it.
pub async fn change(tx: &mut Tx, id: Uuid, changes: &CourseChanges) -> Result<Course> {
    let touched = sqlx::query(
        "update courses
            set title = coalesce($2, title),
                about = coalesce($3, about),
                state = coalesce($4, state),
                updated_at = now()
          where id = $1 and deleted_at is null",
    )
    .bind(id)
    .bind(changes.title.as_deref())
    .bind(changes.about.as_deref())
    .bind(changes.state.as_deref())
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    if touched.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(THERE_IS_NO_COURSE_LIKE_THAT)));
    }

    read(tx, id).await
}

/// Says what order the modules of a course are in.
///
/// The whole order at once, in the caller's transaction, against a constraint
/// that is checked when it commits — so nothing is ever half reordered and no
/// module is ever parked at a number nobody meant.
pub async fn reorder_modules(tx: &mut Tx, course: Uuid, new_order: &[Uuid]) -> Result<Course> {
    let there_now: Vec<Uuid> = sqlx::query_scalar("select id from modules where course_id = $1")
        .bind(course)
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?;

    for (id, place) in in_this_order(&there_now, new_order)? {
        sqlx::query("update modules set place = $2, updated_at = now() where id = $1")
            .bind(id)
            .bind(place)
            .execute(tx.conn())
            .await
            .map_err(Error::internal)?;
    }

    read(tx, course).await
}

/// The same, for the lessons of one module.
pub async fn reorder_lessons(tx: &mut Tx, module: Uuid, new_order: &[Uuid]) -> Result<Vec<Lesson>> {
    let there_now: Vec<Uuid> = sqlx::query_scalar("select id from lessons where module_id = $1")
        .bind(module)
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?;

    for (id, place) in in_this_order(&there_now, new_order)? {
        sqlx::query("update lessons set place = $2, updated_at = now() where id = $1")
            .bind(id)
            .bind(place)
            .execute(tx.conn())
            .await
            .map_err(Error::internal)?;
    }

    lessons(tx, module).await
}

/// Adds a part to a course, at the end.
pub async fn add_module(tx: &mut Tx, course: Uuid, title: &str) -> Result<Module> {
    let after: Option<i32> =
        sqlx::query_scalar("select max(place) from modules where course_id = $1")
            .bind(course)
            .fetch_one(tx.conn())
            .await
            .map_err(Error::internal)?;

    let place = after.map_or(0, |place| place + 1);
    let id = Uuid::now_v7();

    sqlx::query("insert into modules (id, course_id, title, place) values ($1, $2, $3, $4)")
        .bind(id)
        .bind(course)
        .bind(title.trim())
        .bind(place)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    Ok(Module {
        id,
        title: title.trim().to_owned(),
        place,
        lessons: Vec::new(),
    })
}

/// Adds a lesson to a part, at the end.
pub async fn add_lesson(tx: &mut Tx, module: Uuid, title: &str, body: &str) -> Result<Lesson> {
    let after: Option<i32> =
        sqlx::query_scalar("select max(place) from lessons where module_id = $1")
            .bind(module)
            .fetch_one(tx.conn())
            .await
            .map_err(Error::internal)?;

    let place = after.map_or(0, |place| place + 1);
    let id = Uuid::now_v7();

    sqlx::query(
        "insert into lessons (id, module_id, title, body, place) values ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(module)
    .bind(title.trim())
    .bind(body)
    .bind(place)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(Lesson {
        id,
        module_id: module,
        title: title.trim().to_owned(),
        body: body.to_owned(),
        place,
    })
}

/// What may be changed about a lesson.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LessonChanges {
    pub title: Option<String>,
    pub body: Option<String>,
}

/// Changes what a lesson says.
pub async fn change_lesson(tx: &mut Tx, id: Uuid, changes: &LessonChanges) -> Result<Lesson> {
    let row = sqlx::query(
        "update lessons
            set title = coalesce($2, title), body = coalesce($3, body), updated_at = now()
          where id = $1
         returning id, module_id, title, body, place",
    )
    .bind(id)
    .bind(changes.title.as_deref())
    .bind(changes.body.as_deref())
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    row.as_ref()
        .map(a_lesson)
        .transpose()?
        .ok_or_else(|| Error::not_found(Say::of(THERE_IS_NO_LESSON_LIKE_THAT)))
}

/// Somebody learning here.
#[derive(Clone, Debug, Serialize)]
pub struct Student {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub standing: String,
    pub created_at: DateTime<Utc>,
}

/// Everybody learning here.
pub async fn students(tx: &mut Tx, query: &Query) -> Result<Page<Student>> {
    let walk = Walk::new(BY_RECENT, query.after(BY_RECENT)?);
    let mut wheres = vec!["deleted_at is null".to_owned()];

    let cursor = walk.after(1);
    if let Some((sql, _)) = &cursor {
        wheres.push(sql.clone());
    }

    let sql = format!(
        "select id, email, name, standing, created_at from students
          where {} order by {} limit {}",
        wheres.join(" and "),
        walk.order(),
        query.fetch(),
    );

    let mut asking = sqlx::query(&sql);

    if let Some((_, values)) = cursor {
        for value in values {
            asking = asking.bind(value);
        }
    }

    let rows = asking
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?
        .iter()
        .map(|row| {
            Ok(Student {
                id: row.try_get("id").map_err(Error::internal)?,
                email: row.try_get("email").map_err(Error::internal)?,
                name: row.try_get("name").map_err(Error::internal)?,
                standing: row.try_get("standing").map_err(Error::internal)?,
                created_at: row.try_get("created_at").map_err(Error::internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Page::build(query, BY_RECENT, rows, |student| {
        vec![student.created_at.to_rfc3339(), student.id.to_string()]
    })
}

/// Writes to somebody, asking them to take up an account.
pub async fn ask(tx: &mut Tx, email: &str, name: &str) -> Result<Student> {
    let email = mavi_core::email::Email::parse(email)?;

    let row = sqlx::query(
        "insert into students (id, email, name) values ($1, $2, $3)
         returning id, email, name, standing, created_at",
    )
    .bind(Uuid::now_v7())
    .bind(email.as_str())
    .bind(name.trim())
    .fetch_one(tx.conn())
    .await
    .map_err(|cause| match &cause {
        sqlx::Error::Database(db) if db.constraint() == Some("students_address") => {
            Error::conflict(Say::of(SOMEBODY_ALREADY_LEARNS_HERE_UNDER_THAT_ADDRESS))
        }
        _ => Error::internal(cause),
    })?;

    Ok(Student {
        id: row.try_get("id").map_err(Error::internal)?,
        email: row.try_get("email").map_err(Error::internal)?,
        name: row.try_get("name").map_err(Error::internal)?,
        standing: row.try_get("standing").map_err(Error::internal)?,
        created_at: row.try_get("created_at").map_err(Error::internal)?,
    })
}

/// Puts somebody on a course. Twice is once.
pub async fn enrol(tx: &mut Tx, course: Uuid, student: Uuid) -> Result<Uuid> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "insert into enrolments (id, student_id, course_id) values ($1, $2, $3)
         on conflict (student_id, course_id) do nothing
         returning id",
    )
    .bind(Uuid::now_v7())
    .bind(student)
    .bind(course)
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    if let Some(id) = id {
        return Ok(id);
    }

    // Already on it, which is what pressing the button twice means. The one
    // that is already there is the answer.
    sqlx::query_scalar("select id from enrolments where student_id = $1 and course_id = $2")
        .bind(student)
        .bind(course)
        .fetch_optional(tx.conn())
        .await
        .map_err(Error::internal)?
        .ok_or_else(|| Error::not_found(Say::of(NOBODY_HERE_IS_LEARNING_UNDER_THAT)))
}

/// Takes somebody off a course. What they did stays theirs.
pub async fn unenrol(tx: &mut Tx, id: Uuid) -> Result<()> {
    let gone = sqlx::query("delete from enrolments where id = $1")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    if gone.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(
            NOBODY_HERE_IS_LEARNING_UNDER_THAT,
        )));
    }

    Ok(())
}

/// The courses one student is on.
pub async fn learning(tx: &mut Tx, student: Uuid) -> Result<Vec<Course>> {
    let rows = sqlx::query(
        "select c.id, c.slug, c.title, c.about, c.state, c.created_at from enrolments e
           join courses c on c.id = e.course_id
          where e.student_id = $1 and c.deleted_at is null
          order by e.created_at desc",
    )
    .bind(student)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter().map(a_course).collect()
}

/// One lesson, if they are on the course and it is open.
///
/// All three questions are asked in one query and answered by [`may_open`], so
/// there is one place that reads and one place that decides — the arrangement
/// the crate this replaces did not have, which is how a lesson in a closed
/// course stayed open to anybody holding its address.
pub async fn a_students_lesson(tx: &mut Tx, student: Uuid, lesson: Uuid) -> Result<Lesson> {
    let row = sqlx::query(
        "select l.id, l.module_id, l.title, l.body, l.place,
                c.state,
                s.standing,
                exists (
                    select 1 from enrolments e
                     where e.student_id = s.id and e.course_id = c.id
                ) as on_the_course
           from lessons l
           join modules m on m.id = l.module_id
           join courses c on c.id = m.course_id
           join students s on s.id = $1
          where l.id = $2 and c.deleted_at is null and s.deleted_at is null",
    )
    .bind(student)
    .bind(lesson)
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?
    .ok_or_else(|| Error::not_found(Say::of(THERE_IS_NO_LESSON_LIKE_THAT)))?;

    let standing: String = row.try_get("standing").map_err(Error::internal)?;
    let state: String = row.try_get("state").map_err(Error::internal)?;
    let on_the_course: bool = row.try_get("on_the_course").map_err(Error::internal)?;

    let standing = match standing.as_str() {
        "learning" => Standing::Learning,
        "stopped" => Standing::Stopped,
        _ => Standing::Asked,
    };

    may_open(standing, on_the_course, a_state(&state).is_open())?;

    a_lesson(&row)
}

/// Says a lesson is done. Saying it twice is saying it once.
pub async fn done(tx: &mut Tx, student: Uuid, lesson: Uuid) -> Result<DateTime<Utc>> {
    // Read first, which is what applies the three rules: nobody marks done a
    // lesson they could not have opened.
    a_students_lesson(tx, student, lesson).await?;

    let at: DateTime<Utc> = sqlx::query_scalar(
        "insert into done (student_id, lesson_id) values ($1, $2)
         on conflict (student_id, lesson_id) do update set at = done.at
         returning at",
    )
    .bind(student)
    .bind(lesson)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(at)
}

/// Takes a lesson away.
///
/// Gone, and what students had finished goes with it — `done` points at the
/// lesson and a row saying somebody finished something that no longer exists
/// is a row that makes a progress bar read wrong. The rest of what they
/// finished is untouched.
pub async fn remove_lesson(tx: &mut Tx, id: Uuid) -> Result<()> {
    let gone = sqlx::query("delete from lessons where id = $1")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    if gone.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(THERE_IS_NO_LESSON_LIKE_THAT)));
    }

    Ok(())
}

/// Takes a part of a course away, and its lessons with it.
///
/// A lesson lives in a part the way a card lives on a board: not a thing on
/// its own, so leaving them would leave lessons nothing can reach.
pub async fn remove_module(tx: &mut Tx, id: Uuid) -> Result<()> {
    let gone = sqlx::query("delete from modules where id = $1")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    if gone.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(THERE_IS_NO_MODULE_LIKE_THAT)));
    }

    Ok(())
}
