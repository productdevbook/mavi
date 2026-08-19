use std::env;

use chrono::Utc;
use mavi_core::{MaviError, PageRequest, RequestId, SiteContext, SiteId};
use mavi_courses::{
    CourseListFilter, CourseState, CoursesService, CreateCourse, CreateLesson, CreateModule,
    CreateStudent, EnrollStudent, EnrollmentListFilter, LearningCourseListFilter, LessonListFilter,
    ReorderLessons, ReorderModules, StudentActivationInput, UpdateCourse, UpdateLesson,
};
use mavi_storage::Database;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn courses_learning_and_progress_are_site_scoped() {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2).await.expect("database");
    database.migrate().await.expect("migrations");
    let first_site = SiteId::new();
    let second_site = SiteId::new();
    database.ensure_site(first_site).await.expect("first site");
    database
        .ensure_site(second_site)
        .await
        .expect("second site");

    let service = CoursesService;
    let first_context = SiteContext::public(first_site);
    let (course, module_one, module_two, lesson_one, lesson_two, invitation) = {
        let mut transaction = database
            .begin(&first_context)
            .await
            .expect("authoring scope");
        let course = service
            .create_course(
                &mut transaction,
                &first_context,
                &CreateCourse {
                    slug: "rust-foundations".to_owned(),
                    title: "Rust Foundations".to_owned(),
                    about: Some("Learn Rust safely".to_owned()),
                },
            )
            .await
            .expect("course");
        let module_one = service
            .create_module(
                &mut transaction,
                &first_context,
                course.id,
                &CreateModule {
                    title: "Ownership".to_owned(),
                },
            )
            .await
            .expect("first module");
        let module_two = service
            .create_module(
                &mut transaction,
                &first_context,
                course.id,
                &CreateModule {
                    title: "Traits".to_owned(),
                },
            )
            .await
            .expect("second module");
        let lesson_one = service
            .create_lesson(
                &mut transaction,
                &first_context,
                module_one.id,
                &CreateLesson {
                    title: "Borrowing".to_owned(),
                    body: "References have a lifetime.".to_owned(),
                    media_file_id: None,
                },
            )
            .await
            .expect("first lesson");
        let lesson_two = service
            .create_lesson(
                &mut transaction,
                &first_context,
                module_one.id,
                &CreateLesson {
                    title: "Lifetimes".to_owned(),
                    body: "Lifetimes describe relationships.".to_owned(),
                    media_file_id: None,
                },
            )
            .await
            .expect("second lesson");
        let page = service
            .list_lessons(
                &mut transaction,
                &first_context,
                module_one.id,
                &LessonListFilter {
                    page: PageRequest {
                        after: None,
                        limit: Some(1),
                    },
                },
            )
            .await
            .expect("lesson cursor page");
        assert_eq!(page.items.len(), 1);
        assert!(page.next_cursor.is_some());
        let next_page = service
            .list_lessons(
                &mut transaction,
                &first_context,
                module_one.id,
                &LessonListFilter {
                    page: PageRequest {
                        after: page.next_cursor,
                        limit: Some(1),
                    },
                },
            )
            .await
            .expect("lesson cursor continuation");
        assert_eq!(next_page.items.len(), 1);
        assert!(next_page.next_cursor.is_none());
        service
            .reorder_lessons(
                &mut transaction,
                &first_context,
                module_one.id,
                &ReorderLessons {
                    order: vec![lesson_two.id, lesson_one.id],
                },
            )
            .await
            .expect("lesson reorder");
        service
            .reorder_modules(
                &mut transaction,
                &first_context,
                course.id,
                &ReorderModules {
                    order: vec![module_two.id, module_one.id],
                },
            )
            .await
            .expect("module reorder");
        let opened = service
            .update_course(
                &mut transaction,
                &first_context,
                course.id,
                &UpdateCourse {
                    state: Some(CourseState::Open),
                    ..UpdateCourse::default()
                },
            )
            .await
            .expect("open course");
        assert_eq!(opened.state, CourseState::Open);
        let invitation = service
            .create_student(
                &mut transaction,
                &first_context,
                &CreateStudent {
                    email: "student@example.test".to_owned(),
                    name: "A Student".to_owned(),
                },
                Utc::now(),
            )
            .await
            .expect("student invitation");
        transaction.commit().await.expect("authoring commit");
        (
            course, module_one, module_two, lesson_one, lesson_two, invitation,
        )
    };

    let session = {
        let mut transaction = database
            .begin(&first_context)
            .await
            .expect("activation scope");
        let session = service
            .activate_student(
                &mut transaction,
                &first_context,
                &StudentActivationInput {
                    email: "student@example.test".to_owned(),
                    invitation_token: invitation.invitation_token,
                    password: "student-password-long".to_owned(),
                },
                Utc::now(),
            )
            .await
            .expect("student activation");
        assert_eq!(
            session.student.standing,
            mavi_courses::StudentStanding::Learning
        );
        transaction.commit().await.expect("activation commit");
        session
    };

    let student_context = {
        let mut transaction = database.begin(&first_context).await.expect("session scope");
        let caller = service
            .authenticate_student(&mut transaction, &first_context, &session.token, Utc::now())
            .await
            .expect("student session");
        transaction.commit().await.expect("session commit");
        SiteContext::with_caller(first_site, caller, RequestId::new())
    };

    let enrollment = {
        let mut transaction = database
            .begin(&first_context)
            .await
            .expect("enrollment scope");
        let enrollment = service
            .enroll(
                &mut transaction,
                &first_context,
                course.id,
                &EnrollStudent {
                    student_id: session.student.id,
                },
            )
            .await
            .expect("enrollment");
        let duplicate = service
            .enroll(
                &mut transaction,
                &first_context,
                course.id,
                &EnrollStudent {
                    student_id: session.student.id,
                },
            )
            .await
            .expect("idempotent enrollment");
        assert_eq!(duplicate.id, enrollment.id);
        let enrollments = service
            .list_enrollments(
                &mut transaction,
                &first_context,
                course.id,
                &EnrollmentListFilter::default(),
            )
            .await
            .expect("enrollments");
        assert_eq!(enrollments.items.len(), 1);
        transaction.commit().await.expect("enrollment commit");
        enrollment
    };

    {
        let mut transaction = database
            .begin(&student_context)
            .await
            .expect("learning scope");
        let learning = service
            .list_learning_courses(
                &mut transaction,
                &student_context,
                &LearningCourseListFilter::default(),
            )
            .await
            .expect("learning courses");
        assert_eq!(learning.items.len(), 1);
        assert_eq!(learning.items[0].total_lessons, 2);
        assert_eq!(learning.items[0].completed_lessons, 0);
        let lesson = service
            .get_learning_lesson(&mut transaction, &student_context, lesson_one.id)
            .await
            .expect("learning lesson");
        assert!(lesson.completed_at.is_none());
        let first_progress = service
            .complete_lesson(
                &mut transaction,
                &student_context,
                lesson_one.id,
                Utc::now(),
            )
            .await
            .expect("first completion");
        let second_progress = service
            .complete_lesson(
                &mut transaction,
                &student_context,
                lesson_one.id,
                Utc::now() + chrono::Duration::seconds(1),
            )
            .await
            .expect("idempotent completion");
        assert_eq!(first_progress.completed_at, second_progress.completed_at);
        transaction.commit().await.expect("learning commit");
    }

    {
        let mut transaction = database.begin(&first_context).await.expect("close scope");
        let closed = service
            .update_course(
                &mut transaction,
                &first_context,
                course.id,
                &UpdateCourse {
                    state: Some(CourseState::Closed),
                    ..UpdateCourse::default()
                },
            )
            .await
            .expect("close course");
        assert_eq!(closed.state, CourseState::Closed);
        assert!(matches!(
            service
                .update_lesson(
                    &mut transaction,
                    &first_context,
                    lesson_one.id,
                    &UpdateLesson {
                        body: Some("blocked".to_owned()),
                        ..UpdateLesson::default()
                    },
                )
                .await,
            Err(MaviError::Conflict { .. })
        ));
        transaction.commit().await.expect("close commit");
    }

    {
        let mut transaction = database
            .begin(&student_context)
            .await
            .expect("closed learning scope");
        assert!(matches!(
            service
                .get_learning_lesson(&mut transaction, &student_context, lesson_one.id)
                .await,
            Err(MaviError::Forbidden)
        ));
        transaction.commit().await.expect("closed learning commit");
    }

    let second_context = SiteContext::public(second_site);
    let mut second_transaction = database.begin(&second_context).await.expect("second scope");
    assert!(
        service
            .list_courses(
                &mut second_transaction,
                &second_context,
                &CourseListFilter::default(),
            )
            .await
            .expect("second course list")
            .items
            .is_empty()
    );
    assert!(matches!(
        service
            .get_course(&mut second_transaction, &second_context, course.id)
            .await,
        Err(MaviError::NotFound { .. })
    ));
    assert_eq!(enrollment.student_id, session.student.id);
    assert_eq!(module_one.course_id, course.id);
    assert_eq!(module_two.course_id, course.id);
    assert_eq!(lesson_two.module_id, module_one.id);
    second_transaction.commit().await.expect("second commit");
}
