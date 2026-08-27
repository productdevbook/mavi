#![allow(clippy::wildcard_imports)]

use super::super::*;

pub(super) fn form_routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/forms", get(list_forms).post(create_form))
        .route(
            "/api/v1/forms/{id}",
            get(read_form).patch(update_form).delete(delete_form),
        )
        .route("/api/v1/forms/{id}/submissions", get(list_form_submissions))
        .route(
            "/api/v1/forms/{id}/submissions/export",
            get(export_form_submissions),
        )
        .route(
            "/api/v1/forms/{id}/submissions/mark-read",
            post(mark_form_submissions_read),
        )
        .route(
            "/api/v1/form-submissions/{id}",
            delete(delete_form_submission),
        )
        .route("/public/v1/forms/{slug}", get(public_form))
        .route("/public/v1/forms/{slug}/submissions", post(submit_form))
}

async fn list_forms(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<FormListFilter>,
) -> Result<Json<Page<Form>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Forms, Action::View),
        "Form",
        "forms",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let forms = state
        .forms
        .list(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(forms))
}

async fn create_form(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateForm>,
) -> Result<(StatusCode, Json<Form>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Forms, Action::Write),
        "Form",
        "forms",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let form = state
        .forms
        .create(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(form)))
}

async fn read_form(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<mavi_core::FormId>,
) -> Result<Json<Form>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Forms, Action::View),
        "Form",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let form = state
        .forms
        .get(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(form))
}

async fn update_form(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<mavi_core::FormId>,
    Json(input): Json<UpdateForm>,
) -> Result<Json<Form>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Forms, Action::Write),
        "Form",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let form = state
        .forms
        .update(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(form))
}

async fn delete_form(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<mavi_core::FormId>,
) -> Result<StatusCode, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Forms, Action::Delete),
        "Form",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .forms
        .delete(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_form_submissions(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(form_id): Path<mavi_core::FormId>,
    Query(filter): Query<SubmissionListFilter>,
) -> Result<Json<Page<FormSubmission>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Forms, Action::View),
        "Form",
        form_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let submissions = state
        .forms
        .list_submissions(&mut transaction, &context, form_id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(submissions))
}

async fn export_form_submissions(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(form_id): Path<mavi_core::FormId>,
    Query(filter): Query<SubmissionExportFilter>,
) -> Result<Json<FormSubmissionExport>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Forms, Action::View),
        "Form",
        form_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let export = state
        .forms
        .export_submissions(&mut transaction, &context, form_id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(export))
}

async fn mark_form_submissions_read(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(form_id): Path<mavi_core::FormId>,
) -> Result<Json<SeenCount>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Forms, Action::Write),
        "Form",
        form_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let count = state
        .forms
        .mark_read(&mut transaction, &context, form_id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(count))
}

async fn delete_form_submission(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FormSubmissionId>,
) -> Result<StatusCode, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Forms, Action::Delete),
        "FormSubmission",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .forms
        .delete_submission(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn public_form(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(slug): Path<String>,
) -> Result<Json<PublicForm>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let form = state
        .forms
        .public_get(&mut transaction, &context, &slug)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(form))
}

async fn submit_form(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(slug): Path<String>,
    Json(input): Json<SubmitForm>,
) -> Result<(StatusCode, Json<SubmissionReceipt>), HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let receipt = state
        .forms
        .submit(&mut transaction, &context, &slug, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(receipt)))
}
