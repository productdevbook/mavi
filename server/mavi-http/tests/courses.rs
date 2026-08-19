use axum::http::{Method, StatusCode, header::CONTENT_TYPE};
use serde_json::json;

mod support;
use support::{bootstrap, response_bytes, response_json, send, send_raw};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn course_instructor_grants_are_resource_scoped_and_cedar_backed() {
    let app = support::build_app().await;
    let owner_token = bootstrap(&app, "HTTP course instructors test").await;

    let course = send(
        &app,
        Method::POST,
        "/api/v1/courses",
        Some(&owner_token),
        Some(json!({"slug": "cedar-course", "title": "Cedar Course"})),
    )
    .await;
    assert_eq!(course.status(), StatusCode::CREATED);
    let course_id = response_json(course).await["id"]
        .as_str()
        .expect("course id")
        .to_owned();

    let person = send(
        &app,
        Method::POST,
        "/api/v1/people",
        Some(&owner_token),
        Some(json!({
            "email": "instructor@example.com",
            "name": "Course Instructor",
            "password": "long-enough-password",
            "role_ids": []
        })),
    )
    .await;
    assert_eq!(person.status(), StatusCode::CREATED);
    let person = response_json(person).await;
    let person_id = person["id"].as_str().expect("person id").to_owned();
    support::verify_email(&app, &person, "instructor@example.com").await;

    let assigned = send(
        &app,
        Method::PUT,
        &format!("/api/v1/courses/{course_id}/instructors/{person_id}"),
        Some(&owner_token),
        Some(json!({"grants": ["view", "write"]})),
    )
    .await;
    assert_eq!(assigned.status(), StatusCode::OK);
    assert_eq!(
        response_json(assigned).await["grants"],
        json!(["view", "write"])
    );

    let instructor_token = support::login(&app, "instructor@example.com").await;
    let read = send(
        &app,
        Method::GET,
        &format!("/api/v1/courses/{course_id}"),
        Some(&instructor_token),
        None,
    )
    .await;
    assert_eq!(read.status(), StatusCode::OK);

    let module = send(
        &app,
        Method::POST,
        &format!("/api/v1/courses/{course_id}/modules"),
        Some(&instructor_token),
        Some(json!({"title": "Scoped module"})),
    )
    .await;
    assert_eq!(module.status(), StatusCode::CREATED);

    let removed = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/courses/{course_id}/instructors/{person_id}"),
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);

    let denied = send(
        &app,
        Method::GET,
        &format!("/api/v1/courses/{course_id}"),
        Some(&instructor_token),
        None,
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn course_routes_isolate_student_learning_and_protected_media() {
    let app = support::build_app().await;
    let owner_token = bootstrap(&app, "HTTP courses test").await;

    let course = send(
        &app,
        Method::POST,
        "/api/v1/courses",
        Some(&owner_token),
        Some(json!({
            "slug": "rust-foundations",
            "title": "Rust Foundations",
            "about": "Learn Rust safely"
        })),
    )
    .await;
    assert_eq!(course.status(), StatusCode::CREATED);
    let course = response_json(course).await;
    let course_id = course["id"].as_str().expect("course id").to_owned();

    let module = send(
        &app,
        Method::POST,
        &format!("/api/v1/courses/{course_id}/modules"),
        Some(&owner_token),
        Some(json!({"title": "Ownership"})),
    )
    .await;
    assert_eq!(module.status(), StatusCode::CREATED);
    let module_id = response_json(module).await["id"]
        .as_str()
        .expect("module id")
        .to_owned();

    let upload = send_raw(
        &app,
        Method::POST,
        "/api/v1/files?name=intro.png",
        Some(&owner_token),
        "application/octet-stream",
        b"\x89PNG\r\n\x1a\nlesson-media".to_vec(),
    )
    .await;
    assert_eq!(upload.status(), StatusCode::CREATED);
    let file_id = response_json(upload).await["id"]
        .as_str()
        .expect("file id")
        .to_owned();

    let lesson = send(
        &app,
        Method::POST,
        &format!("/api/v1/courses/modules/{module_id}/lessons"),
        Some(&owner_token),
        Some(json!({
            "title": "Borrowing",
            "body": "References have a lifetime.",
            "media_file_id": file_id
        })),
    )
    .await;
    assert_eq!(lesson.status(), StatusCode::CREATED);
    let lesson = response_json(lesson).await;
    let lesson_id = lesson["id"].as_str().expect("lesson id").to_owned();

    let opened = send(
        &app,
        Method::PATCH,
        &format!("/api/v1/courses/{course_id}"),
        Some(&owner_token),
        Some(json!({"state": "open"})),
    )
    .await;
    assert_eq!(opened.status(), StatusCode::OK);

    let invitation = send(
        &app,
        Method::POST,
        "/api/v1/courses/students",
        Some(&owner_token),
        Some(json!({"email": "student@example.test", "name": "A Student"})),
    )
    .await;
    assert_eq!(invitation.status(), StatusCode::CREATED);
    let invitation = response_json(invitation).await;
    let student_id = invitation["student"]["id"]
        .as_str()
        .expect("student id")
        .to_owned();
    let invitation_token = invitation["invitation_token"]
        .as_str()
        .expect("invitation token")
        .to_owned();

    let activation = send(
        &app,
        Method::POST,
        "/public/v1/courses/students/activate",
        None,
        Some(json!({
            "email": "student@example.test",
            "invitation_token": invitation_token,
            "password": "student-password-long"
        })),
    )
    .await;
    assert_eq!(activation.status(), StatusCode::CREATED);
    let activation = response_json(activation).await;
    assert_eq!(activation["student"]["standing"], "learning");
    let student_token = activation["token"]
        .as_str()
        .expect("student token")
        .to_owned();

    let enrollment = send(
        &app,
        Method::POST,
        &format!("/api/v1/courses/{course_id}/enrollments"),
        Some(&owner_token),
        Some(json!({"student_id": student_id})),
    )
    .await;
    assert_eq!(enrollment.status(), StatusCode::CREATED);

    let learning_courses = send(
        &app,
        Method::GET,
        "/student/v1/learning/courses?limit=1",
        Some(&student_token),
        None,
    )
    .await;
    assert_eq!(learning_courses.status(), StatusCode::OK);
    let learning_courses = response_json(learning_courses).await;
    assert_eq!(learning_courses["items"][0]["course_id"], course_id);
    assert_eq!(learning_courses["items"][0]["total_lessons"], 1);

    let student_lesson = send(
        &app,
        Method::GET,
        &format!("/student/v1/learning/lessons/{lesson_id}"),
        Some(&student_token),
        None,
    )
    .await;
    assert_eq!(student_lesson.status(), StatusCode::OK);
    assert_eq!(
        response_json(student_lesson).await["completed_at"],
        json!(null)
    );

    let media = send(
        &app,
        Method::GET,
        &format!("/student/v1/learning/lessons/{lesson_id}/media"),
        Some(&student_token),
        None,
    )
    .await;
    assert_eq!(media.status(), StatusCode::OK);
    assert_eq!(media.headers()[CONTENT_TYPE], "image/png");
    assert_eq!(
        response_bytes(media).await,
        b"\x89PNG\r\n\x1a\nlesson-media"
    );

    let first_done = send(
        &app,
        Method::PUT,
        &format!("/student/v1/learning/lessons/{lesson_id}/done"),
        Some(&student_token),
        None,
    )
    .await;
    assert_eq!(first_done.status(), StatusCode::OK);
    let first_done = response_json(first_done).await;
    let second_done = send(
        &app,
        Method::PUT,
        &format!("/student/v1/learning/lessons/{lesson_id}/done"),
        Some(&student_token),
        None,
    )
    .await;
    assert_eq!(second_done.status(), StatusCode::OK);
    assert_eq!(
        response_json(second_done).await["completed_at"],
        first_done["completed_at"]
    );

    let student_cannot_use_admin_api = send(
        &app,
        Method::GET,
        "/api/v1/courses",
        Some(&student_token),
        None,
    )
    .await;
    assert_eq!(student_cannot_use_admin_api.status(), StatusCode::FORBIDDEN);

    let closed = send(
        &app,
        Method::PATCH,
        &format!("/api/v1/courses/{course_id}"),
        Some(&owner_token),
        Some(json!({"state": "closed"})),
    )
    .await;
    assert_eq!(closed.status(), StatusCode::OK);

    let closed_lesson = send(
        &app,
        Method::GET,
        &format!("/student/v1/learning/lessons/{lesson_id}"),
        Some(&student_token),
        None,
    )
    .await;
    assert_eq!(closed_lesson.status(), StatusCode::FORBIDDEN);
    let closed_media = send(
        &app,
        Method::GET,
        &format!("/student/v1/learning/lessons/{lesson_id}/media"),
        Some(&student_token),
        None,
    )
    .await;
    assert_eq!(closed_media.status(), StatusCode::FORBIDDEN);
}
