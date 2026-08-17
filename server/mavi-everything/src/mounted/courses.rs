use super::helpers::{asking, took_it_away};
use mavi_http::Answered;
// Domain route module: courses

use mavi_api::Who;
use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Db;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;
use uuid::Uuid;

use super::helpers::{THAT_IS_NOT_AN_ID, a_uuid, handling, wrote_about};

/// Courses, who is on them, and what a student reaches.
#[must_use]
pub fn what_it_teaches(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_courses::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "courses.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { courses(&db, &asked).await })
            })),
            "courses.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_course(&db, &asked).await })
            })),
            "courses.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { one_course(&db, &asked).await })
            })),
            "courses.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_course(&db, &asked).await })
            })),
            "courses.reorder" => Some(handling(db, |db, asked| {
                Box::pin(async move { reordered_modules(&db, &asked).await })
            })),
            "modules.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { added_a_module(&db, &asked).await })
            })),
            "modules.reorder" => Some(handling(db, |db, asked| {
                Box::pin(async move { reordered_lessons(&db, &asked).await })
            })),
            "lessons.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { added_a_lesson(&db, &asked).await })
            })),
            "modules.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { took_a_module_away(&db, &asked).await })
            })),
            "lessons.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { took_a_lesson_away(&db, &asked).await })
            })),
            "lessons.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_lesson(&db, &asked).await })
            })),
            "students.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { students(&db, &asked).await })
            })),
            "students.ask" => Some(handling(db, |db, asked| {
                Box::pin(async move { asked_somebody(&db, &asked).await })
            })),
            "enrolments.add" => Some(handling(db, |db, asked| {
                Box::pin(async move { put_on_a_course(&db, &asked).await })
            })),
            "enrolments.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { taken_off_a_course(&db, &asked).await })
            })),
            "learning.mine" => Some(handling(db, |db, asked| {
                Box::pin(async move { what_they_are_on(&db, &asked).await })
            })),
            "learning.lesson" => Some(handling(db, |db, asked| {
                Box::pin(async move { a_students_lesson(&db, &asked).await })
            })),
            "learning.done" => Some(handling(db, |db, asked| {
                Box::pin(async move { marked_done(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            // A student holds no grants at all, so what they reach asks for
            // nothing held — and what they may see is decided by the three
            // questions the store asks, not by a capability.
            let needs = match (endpoint.who, endpoint.changes) {
                (Who::AStudent, _) => None,
                (_, true) => Some(mavi_courses::to_write()),
                (_, false) => Some(mavi_courses::to_read()),
            };

            site = site.mount(endpoint, needs, handler);
        }
    }

    site
}

/// The shelf, the orders, and the basket a visitor brings.
async fn took_a_module_away(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    took_it_away(db, asked, "modules.remove", "module", |tx, id| {
        Box::pin(mavi_courses::store::remove_module(tx, id))
    })
    .await
}

async fn took_a_lesson_away(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    took_it_away(db, asked, "lessons.remove", "lesson", |tx, id| {
        Box::pin(mavi_courses::store::remove_lesson(tx, id))
    })
    .await
}

/// A coupon is reached by its code rather than by an id, because a code is
/// what somebody typed off a poster and what every other coupon endpoint takes.
async fn courses(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_courses::store::list(
        &mut tx,
        asked.query.get("state").map(String::as_str),
        &asking(asked),
    )
    .await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn made_a_course(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_courses::store::NewCourse = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_course")))?;

    let mut tx = db.begin().await?;
    let course = mavi_courses::store::make(&mut tx, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "courses.make",
        "course",
        Some(&course.id.to_string()),
        &serde_json::json!({ "slug": course.slug }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(course).map_err(Error::internal)?,
        receipt,
    ))
}

async fn one_course(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let course = mavi_courses::store::read(&mut tx, a_uuid(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(course).map_err(Error::internal)?,
    ))
}

async fn changed_a_course(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_courses::store::CourseChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_a_course")))?;

    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let course = mavi_courses::store::change(&mut tx, id, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "courses.change",
        "course",
        Some(&id.to_string()),
        &serde_json::json!({ "state": course.state.as_str() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(course).map_err(Error::internal)?,
        receipt,
    ))
}

/// The order somebody dragged things into, as ids.
/// The order somebody dragged things into, as ids.
fn the_order(asked: &Asked) -> Vec<Uuid> {
    asked.body["order"]
        .as_array()
        .map(|order| {
            order
                .iter()
                .filter_map(|id| id.as_str())
                .filter_map(|id| Uuid::parse_str(id).ok())
                .collect()
        })
        .unwrap_or_default()
}

async fn reordered_modules(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    let course = mavi_courses::store::reorder_modules(&mut tx, id, &the_order(asked)).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "courses.reorder",
        "course",
        Some(&id.to_string()),
        &serde_json::json!({ "parts": course.modules.len() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(course).map_err(Error::internal)?,
        receipt,
    ))
}

async fn reordered_lessons(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    let lessons = mavi_courses::store::reorder_lessons(&mut tx, id, &the_order(asked)).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "modules.reorder",
        "module",
        Some(&id.to_string()),
        &serde_json::json!({ "lessons": lessons.len() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::json!({ "id": id, "lessons": lessons }),
        receipt,
    ))
}

async fn added_a_module(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let title = asked.body["title"].as_str().unwrap_or_default().to_owned();
    let course = a_uuid(asked)?;

    let mut tx = db.begin().await?;
    let module = mavi_courses::store::add_module(&mut tx, course, &title).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "modules.make",
        "module",
        Some(&module.id.to_string()),
        &serde_json::json!({ "course": course }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(module).map_err(Error::internal)?,
        receipt,
    ))
}

async fn added_a_lesson(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let title = asked.body["title"].as_str().unwrap_or_default().to_owned();
    let body = asked.body["body"].as_str().unwrap_or_default().to_owned();
    let module = a_uuid(asked)?;

    let mut tx = db.begin().await?;
    let lesson = mavi_courses::store::add_lesson(&mut tx, module, &title, &body).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "lessons.make",
        "lesson",
        Some(&lesson.id.to_string()),
        &serde_json::json!({ "module": module }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(lesson).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changed_a_lesson(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_courses::store::LessonChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_a_lesson")))?;

    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let lesson = mavi_courses::store::change_lesson(&mut tx, id, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "lessons.change",
        "lesson",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(lesson).map_err(Error::internal)?,
        receipt,
    ))
}

async fn students(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_courses::store::students(&mut tx, &asking(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn asked_somebody(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let email = asked.body["email"].as_str().unwrap_or_default().to_owned();
    let name = asked.body["name"].as_str().unwrap_or_default().to_owned();

    let mut tx = db.begin().await?;
    let student = mavi_courses::store::ask(&mut tx, &email, &name).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "students.ask",
        "student",
        Some(&student.id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(student).map_err(Error::internal)?,
        receipt,
    ))
}

async fn put_on_a_course(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let course = a_uuid(asked)?;
    let student = asked.body["student"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let mut tx = db.begin().await?;
    let id = mavi_courses::store::enrol(&mut tx, course, student).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "enrolments.add",
        "enrolment",
        Some(&id.to_string()),
        &serde_json::json!({ "course": course, "student": student }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::json!({ "id": id, "course": course, "student": student }),
        receipt,
    ))
}

async fn taken_off_a_course(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    mavi_courses::store::unenrol(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "enrolments.remove",
        "enrolment",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

/// Which student is asking. A student holds no grants, so this is the whole of
/// who they are.
/// Which student is asking. A student holds no grants, so this is the whole of
/// who they are.
fn a_student(asked: &Asked) -> Result<Uuid> {
    asked
        .caller
        .id()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))
}

async fn what_they_are_on(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let courses = mavi_courses::store::learning(&mut tx, a_student(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(courses).map_err(Error::internal)?,
    ))
}

async fn a_students_lesson(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let lesson =
        mavi_courses::store::a_students_lesson(&mut tx, a_student(asked)?, a_uuid(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(lesson).map_err(Error::internal)?,
    ))
}

async fn marked_done(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let student = a_student(asked)?;
    let lesson = a_uuid(asked)?;

    let mut tx = db.begin().await?;
    let at = mavi_courses::store::done(&mut tx, student, lesson).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "learning.done",
        "lesson",
        Some(&lesson.to_string()),
        &serde_json::json!({ "student": student }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::json!({ "lesson": lesson, "at": at }),
        receipt,
    ))
}
