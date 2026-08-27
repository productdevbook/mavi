#![allow(clippy::wildcard_imports)]

use super::super::*;

pub(super) fn course_routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/courses", get(list_courses).post(create_course))
        .route(
            "/api/v1/courses/{id}",
            get(read_course).patch(update_course).delete(delete_course),
        )
        .route(
            "/api/v1/courses/{course_id}/instructors",
            get(list_course_instructors),
        )
        .route(
            "/api/v1/courses/{course_id}/instructors/{person_id}",
            put(replace_course_instructor).delete(remove_course_instructor),
        )
        .route(
            "/api/v1/courses/{id}/modules/order",
            put(reorder_course_modules),
        )
        .route("/api/v1/courses/{id}/modules", post(create_course_module))
        .route(
            "/api/v1/courses/modules/{id}",
            get(read_course_module)
                .patch(update_course_module)
                .delete(delete_course_module),
        )
        .route(
            "/api/v1/courses/modules/{id}/lessons",
            get(list_course_lessons).post(create_course_lesson),
        )
        .route(
            "/api/v1/courses/modules/{id}/lessons/order",
            put(reorder_course_lessons),
        )
        .route(
            "/api/v1/courses/lessons/{id}",
            patch(update_course_lesson).delete(delete_course_lesson),
        )
        .route(
            "/api/v1/courses/students",
            get(list_course_students).post(create_course_student),
        )
        .route(
            "/api/v1/courses/students/{id}",
            axum::routing::patch(update_course_student).delete(delete_course_student),
        )
        .route(
            "/api/v1/courses/students/{id}/invite",
            post(reissue_course_student_invite),
        )
        .route(
            "/api/v1/courses/{course_id}/enrollments",
            get(list_course_enrollments).post(enroll_course_student),
        )
        .route(
            "/api/v1/courses/enrollments/{id}",
            delete(unenroll_course_student),
        )
        .route(
            "/public/v1/courses/students/activate",
            post(activate_course_student),
        )
        .route(
            "/public/v1/courses/students/sessions",
            post(login_course_student),
        )
        .route("/student/v1/auth/session", delete(logout_course_student))
        .route("/student/v1/learning/courses", get(list_learning_courses))
        .route(
            "/student/v1/learning/courses/{id}",
            get(read_learning_course),
        )
        .route(
            "/student/v1/learning/lessons/{id}",
            get(read_learning_lesson),
        )
        .route(
            "/student/v1/learning/lessons/{id}/media",
            get(read_learning_lesson_media),
        )
        .route(
            "/student/v1/learning/lessons/{id}/done",
            put(complete_learning_lesson),
        )
}

async fn list_courses(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<CourseListFilter>,
) -> Result<Json<Page<CourseSummary>>, HttpError> {
    require_courses_grant(&state, &context, Action::View, "Course", "courses")?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let courses = state
        .courses
        .list_courses(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(courses))
}

async fn create_course(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateCourse>,
) -> Result<(StatusCode, Json<Course>), HttpError> {
    require_courses_grant(&state, &context, Action::Write, "Course", "courses")?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let course = state
        .courses
        .create_course(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(course)))
}

async fn list_course_instructors(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(course_id): Path<CourseId>,
    Query(filter): Query<CourseInstructorListFilter>,
) -> Result<Json<Page<mavi_courses::CourseInstructor>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Courses, Action::View),
        "Course",
        course_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let instructors = state
        .courses
        .list_instructors(&mut transaction, &context, course_id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(instructors))
}

async fn replace_course_instructor(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path((course_id, person_id)): Path<(CourseId, PersonId)>,
    Json(input): Json<ReplaceCourseInstructor>,
) -> Result<Json<mavi_courses::CourseInstructor>, HttpError> {
    require_courses_grant(
        &state,
        &context,
        Action::Write,
        "Course",
        course_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let instructor = state
        .courses
        .replace_instructor(&mut transaction, &context, course_id, person_id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(instructor))
}

async fn remove_course_instructor(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path((course_id, person_id)): Path<(CourseId, PersonId)>,
) -> Result<StatusCode, HttpError> {
    require_courses_grant(
        &state,
        &context,
        Action::Write,
        "Course",
        course_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .courses
        .remove_instructor(&mut transaction, &context, course_id, person_id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn read_course(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<CourseId>,
) -> Result<Json<Course>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::View,
        id,
        "Course",
        id.to_string(),
    )
    .await?;
    let course = state
        .courses
        .get_course(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(course))
}

async fn update_course(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<CourseId>,
    Json(input): Json<UpdateCourse>,
) -> Result<Json<Course>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::Write,
        id,
        "Course",
        id.to_string(),
    )
    .await?;
    let course = state
        .courses
        .update_course(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(course))
}

async fn delete_course(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<CourseId>,
) -> Result<StatusCode, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::Delete,
        id,
        "Course",
        id.to_string(),
    )
    .await?;
    state
        .courses
        .delete_course(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reorder_course_modules(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<CourseId>,
    Json(input): Json<ReorderModules>,
) -> Result<Json<Course>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::Write,
        id,
        "Course",
        id.to_string(),
    )
    .await?;
    let course = state
        .courses
        .reorder_modules(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(course))
}

async fn create_course_module(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<CourseId>,
    Json(input): Json<CreateModule>,
) -> Result<(StatusCode, Json<Module>), HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::Write,
        id,
        "Course",
        id.to_string(),
    )
    .await?;
    let module = state
        .courses
        .create_module(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(module)))
}

async fn read_course_module(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ModuleId>,
) -> Result<Json<Module>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let course_id = state
        .courses
        .course_id_for_module(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::View,
        course_id,
        "CourseModule",
        id.to_string(),
    )
    .await?;
    let module = state
        .courses
        .get_module(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(module))
}

async fn update_course_module(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ModuleId>,
    Json(input): Json<UpdateModule>,
) -> Result<Json<Module>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let course_id = state
        .courses
        .course_id_for_module(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::Write,
        course_id,
        "CourseModule",
        id.to_string(),
    )
    .await?;
    let module = state
        .courses
        .update_module(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(module))
}

async fn delete_course_module(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ModuleId>,
) -> Result<StatusCode, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let course_id = state
        .courses
        .course_id_for_module(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::Delete,
        course_id,
        "CourseModule",
        id.to_string(),
    )
    .await?;
    state
        .courses
        .delete_module(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_course_lessons(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ModuleId>,
    Query(filter): Query<LessonListFilter>,
) -> Result<Json<Page<Lesson>>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let course_id = state
        .courses
        .course_id_for_module(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::View,
        course_id,
        "CourseModule",
        id.to_string(),
    )
    .await?;
    let lessons = state
        .courses
        .list_lessons(&mut transaction, &context, id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(lessons))
}

async fn reorder_course_lessons(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ModuleId>,
    Json(input): Json<ReorderLessons>,
) -> Result<Json<Module>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let course_id = state
        .courses
        .course_id_for_module(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::Write,
        course_id,
        "CourseModule",
        id.to_string(),
    )
    .await?;
    let module = state
        .courses
        .reorder_lessons(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(module))
}

async fn create_course_lesson(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ModuleId>,
    Json(input): Json<CreateLesson>,
) -> Result<(StatusCode, Json<Lesson>), HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let course_id = state
        .courses
        .course_id_for_module(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::Write,
        course_id,
        "CourseModule",
        id.to_string(),
    )
    .await?;
    let lesson = state
        .courses
        .create_lesson(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(lesson)))
}

async fn update_course_lesson(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<LessonId>,
    Json(input): Json<UpdateLesson>,
) -> Result<Json<Lesson>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let course_id = state
        .courses
        .course_id_for_lesson(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::Write,
        course_id,
        "CourseLesson",
        id.to_string(),
    )
    .await?;
    let lesson = state
        .courses
        .update_lesson(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(lesson))
}

async fn delete_course_lesson(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<LessonId>,
) -> Result<StatusCode, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let course_id = state
        .courses
        .course_id_for_lesson(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::Delete,
        course_id,
        "CourseLesson",
        id.to_string(),
    )
    .await?;
    state
        .courses
        .delete_lesson(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_course_students(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<StudentListFilter>,
) -> Result<Json<Page<Student>>, HttpError> {
    require_courses_grant(&state, &context, Action::View, "CourseStudent", "students")?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let students = state
        .courses
        .list_students(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(students))
}

async fn create_course_student(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateStudent>,
) -> Result<(StatusCode, Json<StudentInvitation>), HttpError> {
    require_courses_grant(&state, &context, Action::Write, "CourseStudent", "students")?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let student = state
        .courses
        .create_student(&mut transaction, &context, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(student)))
}

async fn reissue_course_student_invite(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<StudentId>,
) -> Result<Json<StudentInvitation>, HttpError> {
    require_courses_grant(
        &state,
        &context,
        Action::Write,
        "CourseStudent",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let student = state
        .courses
        .reissue_invitation(&mut transaction, &context, id, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(student))
}

async fn update_course_student(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<StudentId>,
    Json(input): Json<UpdateStudent>,
) -> Result<Json<Student>, HttpError> {
    require_courses_grant(
        &state,
        &context,
        Action::Write,
        "CourseStudent",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let student = state
        .courses
        .update_student(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(student))
}

async fn delete_course_student(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<StudentId>,
) -> Result<StatusCode, HttpError> {
    require_courses_grant(
        &state,
        &context,
        Action::Delete,
        "CourseStudent",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .courses
        .delete_student(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_course_enrollments(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(course_id): Path<CourseId>,
    Query(filter): Query<EnrollmentListFilter>,
) -> Result<Json<Page<Enrollment>>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::View,
        course_id,
        "Course",
        course_id.to_string(),
    )
    .await?;
    let enrollments = state
        .courses
        .list_enrollments(&mut transaction, &context, course_id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(enrollments))
}

async fn enroll_course_student(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(course_id): Path<CourseId>,
    Json(input): Json<EnrollStudent>,
) -> Result<(StatusCode, Json<Enrollment>), HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::Write,
        course_id,
        "Course",
        course_id.to_string(),
    )
    .await?;
    let enrollment = state
        .courses
        .enroll(&mut transaction, &context, course_id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(enrollment)))
}

async fn unenroll_course_student(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<EnrollmentId>,
) -> Result<StatusCode, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let course_id = state
        .courses
        .course_id_for_enrollment(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    require_course_grant(
        &state,
        &context,
        &mut transaction,
        Action::Delete,
        course_id,
        "CourseEnrollment",
        id.to_string(),
    )
    .await?;
    state
        .courses
        .unenroll(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn activate_course_student(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<StudentActivationInput>,
) -> Result<(StatusCode, Json<StudentSessionCreated>), HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let session = state
        .courses
        .activate_student(&mut transaction, &context, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(session)))
}

async fn login_course_student(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<StudentLoginInput>,
) -> Result<(StatusCode, Json<StudentSessionCreated>), HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let session = state
        .courses
        .login_student(&mut transaction, &context, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(session)))
}

async fn logout_course_student(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
) -> Result<StatusCode, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .courses
        .logout_student(&mut transaction, &context, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_learning_courses(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<LearningCourseListFilter>,
) -> Result<Json<Page<LearningCourse>>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let courses = state
        .courses
        .list_learning_courses(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(courses))
}

async fn read_learning_course(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<CourseId>,
) -> Result<Json<LearningCourseDetail>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let course = state
        .courses
        .get_learning_course(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(course))
}

async fn read_learning_lesson(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<LessonId>,
) -> Result<Json<LearningLesson>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let lesson = state
        .courses
        .get_learning_lesson(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(lesson))
}

async fn read_learning_lesson_media(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<LessonId>,
) -> Result<Response, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let lesson = state
        .courses
        .get_learning_lesson(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    let file_id = lesson
        .lesson
        .media_file_id
        .ok_or(HttpError(MaviError::NotFound {
            resource: "course_lesson_media",
        }))?;
    let (file, bytes) = state
        .media
        .read_bytes(
            &mut transaction,
            &context,
            state.file_store.as_ref(),
            file_id,
        )
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, file.mime)
        .header(CACHE_CONTROL, "private, no-store")
        .header("x-content-type-options", "nosniff")
        .body(Body::from(bytes))
        .map_err(|_| HttpError(MaviError::Internal))
}

async fn complete_learning_lesson(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<LessonId>,
) -> Result<Json<Progress>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let progress = state
        .courses
        .complete_lesson(&mut transaction, &context, id, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(progress))
}
