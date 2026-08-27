#![allow(clippy::wildcard_imports)]

use super::super::*;

pub(super) fn content_routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/content-types", get(list_content_types))
        .route(
            "/api/v1/content-types/{kind}",
            put(upsert_content_type).delete(delete_content_type),
        )
        .route("/api/v1/terms", get(list_terms).post(create_term))
        .route(
            "/api/v1/terms/{id}",
            get(read_term).patch(update_term).delete(delete_term),
        )
        .route("/api/v1/terms/{id}/content", get(list_term_content))
        .route(
            "/api/v1/content/{id}/terms",
            get(list_content_terms).put(replace_content_terms),
        )
        .route(
            "/api/v1/content/{id}/revisions",
            get(list_content_revisions),
        )
        .route(
            "/api/v1/content/{id}/revisions/{revision}",
            get(read_content_revision),
        )
        .route(
            "/api/v1/content/{id}/revisions/{revision}/restore",
            post(restore_content_revision),
        )
        .route(
            "/api/v1/content/{id}",
            get(read_content)
                .patch(update_content)
                .delete(trash_content),
        )
        .route("/api/v1/content", get(list_content).post(create_content))
        .route("/api/v1/content/{id}/publish", post(publish_content))
        .route("/api/v1/content/{id}/schedule", post(schedule_content))
        .route("/api/v1/content/{id}/archive", post(archive_content))
        .route("/api/v1/content/{id}/restore", post(restore_content))
        .route("/public/v1/content/{slug}", get(public_content))
        .route("/public/v1/terms/{kind}/{slug}", get(public_term_archive))
}

pub(super) fn media_routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/files", get(list_files).post(upload_file))
        .route("/api/v1/files/{id}", get(read_file).delete(delete_file))
        .route("/api/v1/files/{id}/content", get(download_file))
        .route("/api/v1/files/{id}/variants", get(list_file_variants))
        .route(
            "/api/v1/files/{id}/variants/{preset}/content",
            get(download_file_variant),
        )
        .route("/public/v1/files/{id}", get(public_file))
        .route(
            "/public/v1/files/{id}/variants/{preset}",
            get(public_file_variant),
        )
}

pub(super) fn design_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/api/v1/design/changes",
            get(list_design_changes).post(start_design_change),
        )
        .route("/api/v1/design/changes/{id}", get(read_design_change))
        .route("/api/v1/design/changes/{id}/files", get(list_design_files))
        .route(
            "/api/v1/design/changes/{id}/file",
            get(read_design_file)
                .put(write_design_file)
                .delete(remove_design_file),
        )
        .route(
            "/api/v1/design/changes/{id}/builds",
            get(list_design_builds).post(create_design_build),
        )
        .route(
            "/api/v1/design/changes/{id}/publish",
            post(publish_design_change),
        )
        .route(
            "/api/v1/design/changes/{id}/rollback",
            post(rollback_design_change),
        )
        .route(
            "/preview/v1/design/{build_id}/{*path}",
            get(preview_design_asset),
        )
        .route("/public/v1/site/{*path}", get(public_design_asset))
}

async fn list_content_types(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<ContentTypeListFilter>,
) -> Result<Json<Page<ContentType>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Content, Action::View),
        "ContentType",
        "content_type_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let content_types = state
        .content
        .list_content_types(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(content_types))
}

async fn upsert_content_type(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(kind): Path<String>,
    Json(input): Json<DeclareContentType>,
) -> Result<Json<ContentType>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Content, Action::Write),
        "ContentType",
        kind.clone(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let content_type = state
        .content
        .upsert_content_type(&mut transaction, &context, &kind, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(content_type))
}

async fn delete_content_type(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(kind): Path<String>,
) -> Result<StatusCode, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Content, Action::Delete),
        "ContentType",
        kind.clone(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .content
        .delete_content_type(&mut transaction, &context, &kind)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_terms(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<TermListFilter>,
) -> Result<Json<Page<Term>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Taxonomy, Action::View),
        "TaxonomyTerm",
        "terms_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let terms = state
        .taxonomy
        .list_terms(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(terms))
}

async fn create_term(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateTerm>,
) -> Result<(StatusCode, Json<Term>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Taxonomy, Action::Write),
        "TaxonomyTerm",
        "terms_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let term = state
        .taxonomy
        .create_term(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(term)))
}

async fn read_term(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<TermId>,
) -> Result<Json<Term>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Taxonomy, Action::View),
        "TaxonomyTerm",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let term = state
        .taxonomy
        .get_term(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(term))
}

async fn update_term(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<TermId>,
    Json(input): Json<UpdateTerm>,
) -> Result<Json<Term>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Taxonomy, Action::Write),
        "TaxonomyTerm",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let term = state
        .taxonomy
        .update_term(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(term))
}

async fn delete_term(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<TermId>,
) -> Result<StatusCode, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Taxonomy, Action::Delete),
        "TaxonomyTerm",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .taxonomy
        .delete_term(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_content_terms(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
) -> Result<Json<Vec<Term>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Taxonomy, Action::View),
        "Content",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let terms = state
        .taxonomy
        .list_content_terms(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(terms))
}

async fn replace_content_terms(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
    Json(input): Json<ReplaceContentTerms>,
) -> Result<Json<Vec<Term>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Taxonomy, Action::Write),
        "Content",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let terms = state
        .taxonomy
        .replace_content_terms(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(terms))
}

async fn list_term_content(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<TermId>,
    Query(filter): Query<ContentTermAssignmentListFilter>,
) -> Result<Json<Page<ContentTermAssignment>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Taxonomy, Action::View),
        "TaxonomyTerm",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let assignments = state
        .taxonomy
        .list_term_content(&mut transaction, &context, id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(assignments))
}

async fn list_files(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<FileListFilter>,
) -> Result<Json<Page<FileRecord>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Media, Action::View),
        "File",
        "files_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let files = state
        .media
        .list(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(files))
}

async fn upload_file(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(query): Query<UploadFileQuery>,
    body: Bytes,
) -> Result<(StatusCode, Json<FileRecord>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Media, Action::Write),
        "File",
        "files_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let file = state
        .media
        .upload(
            &mut transaction,
            &context,
            state.file_store.as_ref(),
            &query.name,
            query.visibility,
            body.to_vec(),
        )
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(file)))
}

async fn read_file(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FileId>,
) -> Result<Json<FileRecord>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Media, Action::View),
        "File",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let file = state
        .media
        .get(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(file))
}

async fn download_file(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FileId>,
) -> Result<Response, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Media, Action::View),
        "File",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let (file, bytes) = state
        .media
        .read_bytes(&mut transaction, &context, state.file_store.as_ref(), id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    media_response(file, bytes, false)
}

async fn list_file_variants(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FileId>,
    Query(filter): Query<FileVariantListFilter>,
) -> Result<Json<Page<FileVariant>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Media, Action::View),
        "File",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let variants = state
        .media
        .list_variants(&mut transaction, &context, id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(variants))
}

async fn download_file_variant(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path((id, preset)): Path<(FileId, VariantPreset)>,
) -> Result<Response, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Media, Action::View),
        "File",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let (variant, bytes) = state
        .media
        .read_variant_bytes(
            &mut transaction,
            &context,
            state.file_store.as_ref(),
            id,
            preset,
            false,
        )
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    media_content_response(variant.mime, bytes, false)
}

async fn public_file(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FileId>,
) -> Result<Response, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let (file, bytes) = state
        .media
        .read_public_bytes(&mut transaction, &context, state.file_store.as_ref(), id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    media_response(file, bytes, true)
}

async fn public_file_variant(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path((id, preset)): Path<(FileId, VariantPreset)>,
) -> Result<Response, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let (variant, bytes) = state
        .media
        .read_variant_bytes(
            &mut transaction,
            &context,
            state.file_store.as_ref(),
            id,
            preset,
            true,
        )
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    media_content_response(variant.mime, bytes, true)
}

fn media_response(file: FileRecord, bytes: Vec<u8>, public: bool) -> Result<Response, HttpError> {
    media_content_response(file.mime, bytes, public)
}

fn media_content_response(
    mime: String,
    bytes: Vec<u8>,
    public: bool,
) -> Result<Response, HttpError> {
    let cache_control = if public {
        "public, max-age=31536000, immutable"
    } else {
        "private, no-store"
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, mime)
        .header(CONTENT_LENGTH, bytes.len())
        .header(CACHE_CONTROL, cache_control)
        .header("content-disposition", "inline")
        .header("x-content-type-options", "nosniff")
        .body(Body::from(bytes))
        .map_err(|_| HttpError(MaviError::Internal))
}

async fn delete_file(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FileId>,
) -> Result<StatusCode, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Media, Action::Delete),
        "File",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .media
        .trash(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_content_revisions(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
    Query(filter): Query<ContentRevisionListFilter>,
) -> Result<Json<Page<ContentRevision>>, HttpError> {
    require_grant(
        &state,
        &context,
        Grant::new(Capability::Content, Action::View),
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let revisions = state
        .content
        .list_revisions(&mut transaction, &context, id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(revisions))
}

async fn read_content_revision(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path((id, revision)): Path<(ContentId, u32)>,
) -> Result<Json<ContentRevision>, HttpError> {
    require_grant(
        &state,
        &context,
        Grant::new(Capability::Content, Action::View),
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let revision = state
        .content
        .read_revision(&mut transaction, &context, id, revision)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(revision))
}

async fn restore_content_revision(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path((id, revision)): Path<(ContentId, u32)>,
) -> Result<Json<Content>, HttpError> {
    require_grant(
        &state,
        &context,
        Grant::new(Capability::Content, Action::Write),
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let entry = state
        .content
        .restore_revision(&mut transaction, &context, id, revision, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(entry))
}

async fn read_content(
    State(state): State<HttpState>,
    Extension(site_context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
) -> Result<Json<Content>, HttpError> {
    require_grant(
        &state,
        &site_context,
        Grant::new(Capability::Content, Action::View),
        id.to_string(),
    )?;
    let mut transaction = state
        .runtime
        .begin(&site_context)
        .await
        .map_err(HttpError)?;
    let entry = state
        .content
        .get(&mut transaction, &site_context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(entry))
}

async fn list_content(
    State(state): State<HttpState>,
    Extension(site_context): Extension<SiteContext>,
    Query(filter): Query<ContentListFilter>,
) -> Result<Json<Page<Content>>, HttpError> {
    require_grant(
        &state,
        &site_context,
        Grant::new(Capability::Content, Action::View),
        "content_collection",
    )?;
    let mut transaction = state
        .runtime
        .begin(&site_context)
        .await
        .map_err(HttpError)?;
    let page = state
        .content
        .list(&mut transaction, &site_context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn create_content(
    State(state): State<HttpState>,
    Extension(site_context): Extension<SiteContext>,
    Json(input): Json<CreateContent>,
) -> Result<(StatusCode, Json<Content>), HttpError> {
    require_grant(
        &state,
        &site_context,
        Grant::new(Capability::Content, Action::Write),
        "content_collection",
    )?;
    if !matches!(&input.publication, PublicationInput::Draft) {
        require_grant(
            &state,
            &site_context,
            Grant::new(Capability::Publish, Action::Write),
            "content_collection",
        )?;
    }
    let mut transaction = state
        .runtime
        .begin(&site_context)
        .await
        .map_err(HttpError)?;
    let entry = state
        .content
        .create(&mut transaction, &site_context, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    state
        .content
        .enqueue_scheduled_publish(&mut transaction, &site_context, &state.jobs, &entry)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(entry)))
}

async fn update_content(
    State(state): State<HttpState>,
    Extension(site_context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
    Json(input): Json<UpdateContent>,
) -> Result<Json<Content>, HttpError> {
    require_grant(
        &state,
        &site_context,
        Grant::new(Capability::Content, Action::Write),
        id.to_string(),
    )?;
    if let Some(publication) = input.publication.as_ref()
        && !matches!(publication, PublicationInput::Draft)
    {
        require_grant(
            &state,
            &site_context,
            Grant::new(Capability::Publish, Action::Write),
            id.to_string(),
        )?;
    }
    let mut transaction = state
        .runtime
        .begin(&site_context)
        .await
        .map_err(HttpError)?;
    let entry = state
        .content
        .update(&mut transaction, &site_context, id, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    state
        .content
        .enqueue_scheduled_publish(&mut transaction, &site_context, &state.jobs, &entry)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(entry))
}

async fn publish_content(
    State(state): State<HttpState>,
    Extension(site_context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
) -> Result<Json<Content>, HttpError> {
    require_grant(
        &state,
        &site_context,
        Grant::new(Capability::Publish, Action::Write),
        id.to_string(),
    )?;
    let mut transaction = state
        .runtime
        .begin(&site_context)
        .await
        .map_err(HttpError)?;
    let entry = state
        .content
        .publish(&mut transaction, &site_context, id, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(entry))
}

async fn schedule_content(
    State(state): State<HttpState>,
    Extension(site_context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
    Json(input): Json<ScheduleContent>,
) -> Result<Json<Content>, HttpError> {
    require_grant(
        &state,
        &site_context,
        Grant::new(Capability::Publish, Action::Write),
        id.to_string(),
    )?;
    let mut transaction = state
        .runtime
        .begin(&site_context)
        .await
        .map_err(HttpError)?;
    let entry = state
        .content
        .schedule(&mut transaction, &site_context, id, input.at, Utc::now())
        .await
        .map_err(HttpError)?;
    state
        .content
        .enqueue_scheduled_publish(&mut transaction, &site_context, &state.jobs, &entry)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(entry))
}

async fn archive_content(
    State(state): State<HttpState>,
    Extension(site_context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
) -> Result<Json<Content>, HttpError> {
    require_grant(
        &state,
        &site_context,
        Grant::new(Capability::Publish, Action::Write),
        id.to_string(),
    )?;
    let mut transaction = state
        .runtime
        .begin(&site_context)
        .await
        .map_err(HttpError)?;
    let entry = state
        .content
        .archive(&mut transaction, &site_context, id, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(entry))
}

async fn trash_content(
    State(state): State<HttpState>,
    Extension(site_context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
) -> Result<StatusCode, HttpError> {
    require_grant(
        &state,
        &site_context,
        Grant::new(Capability::Trash, Action::Delete),
        id.to_string(),
    )?;
    let mut transaction = state
        .runtime
        .begin(&site_context)
        .await
        .map_err(HttpError)?;
    state
        .content
        .trash(&mut transaction, &site_context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn restore_content(
    State(state): State<HttpState>,
    Extension(site_context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
) -> Result<Json<Content>, HttpError> {
    require_grant(
        &state,
        &site_context,
        Grant::new(Capability::Trash, Action::Write),
        id.to_string(),
    )?;
    let mut transaction = state
        .runtime
        .begin(&site_context)
        .await
        .map_err(HttpError)?;
    let entry = state
        .content
        .restore(&mut transaction, &site_context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(entry))
}

#[derive(Debug, Deserialize)]
struct PublicContentQuery {
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PublicTermArchiveQuery {
    language: Option<String>,
    #[serde(flatten)]
    page: PageRequest,
}

async fn public_content(
    State(state): State<HttpState>,
    Extension(site_context): Extension<SiteContext>,
    Path(slug): Path<String>,
    Query(query): Query<PublicContentQuery>,
) -> Result<Json<Content>, HttpError> {
    let mut transaction = state
        .runtime
        .begin(&site_context)
        .await
        .map_err(HttpError)?;
    let languages = state
        .settings
        .public_language_candidates(&mut transaction, &site_context, query.language.as_deref())
        .await
        .map_err(HttpError)?;
    let entry = state
        .content
        .public_get_any(&mut transaction, &site_context, &languages, &slug)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(entry))
}

async fn public_term_archive(
    State(state): State<HttpState>,
    Extension(site_context): Extension<SiteContext>,
    Path((kind, slug)): Path<(String, String)>,
    Query(query): Query<PublicTermArchiveQuery>,
) -> Result<Json<Page<Content>>, HttpError> {
    let mut transaction = state
        .runtime
        .begin(&site_context)
        .await
        .map_err(HttpError)?;
    let languages = state
        .settings
        .public_language_candidates(&mut transaction, &site_context, query.language.as_deref())
        .await
        .map_err(HttpError)?;
    let term = state
        .taxonomy
        .public_get_any(&mut transaction, &site_context, &languages, &kind, &slug)
        .await
        .map_err(HttpError)?;
    let content = state
        .content
        .public_list_for_term(
            &mut transaction,
            &site_context,
            term.id,
            &term.language,
            &query.page,
        )
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(content))
}

async fn list_design_changes(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<DesignChangeListFilter>,
) -> Result<Json<Page<DesignChange>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Design, Action::View),
        "DesignChange",
        "design_changes",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let changes = state
        .design
        .list_changes(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(changes))
}

async fn start_design_change(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<StartDesignChange>,
) -> Result<(StatusCode, Json<DesignChange>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Design, Action::Write),
        "DesignChange",
        "design_changes",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let change = state
        .design
        .start_change(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(change)))
}

async fn read_design_change(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<DesignChangeId>,
) -> Result<Json<DesignChange>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Design, Action::View),
        "DesignChange",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let change = state
        .design
        .get_change(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(change))
}

async fn list_design_files(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
    Query(filter): Query<DesignFileListFilter>,
) -> Result<Json<Page<mavi_design::DesignFileSummary>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Design, Action::View),
        "DesignChange",
        change_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let files = state
        .design
        .list_files(&mut transaction, &context, change_id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(files))
}

async fn read_design_file(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
    Query(query): Query<DesignFileQuery>,
) -> Result<Json<DesignFile>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Design, Action::View),
        "DesignChange",
        change_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let file = state
        .design
        .read_file(&mut transaction, &context, change_id, &query.path)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(file))
}

async fn write_design_file(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
    Json(input): Json<DesignFileInput>,
) -> Result<Json<DesignFile>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Design, Action::Write),
        "DesignChange",
        change_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let file = state
        .design
        .write_file(&mut transaction, &context, change_id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(file))
}

async fn remove_design_file(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
    Query(query): Query<DesignFileQuery>,
) -> Result<StatusCode, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Design, Action::Delete),
        "DesignChange",
        change_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .design
        .remove_file(&mut transaction, &context, change_id, &query.path)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_design_builds(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
    Query(filter): Query<DesignBuildListFilter>,
) -> Result<Json<Page<DesignBuild>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Design, Action::View),
        "DesignChange",
        change_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let builds = state
        .design
        .list_builds(&mut transaction, &context, change_id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(builds))
}

async fn create_design_build(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
) -> Result<(StatusCode, Json<DesignBuild>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Design, Action::Write),
        "DesignChange",
        change_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let request = state
        .design
        .start_build(&mut transaction, &context, change_id)
        .await
        .map_err(HttpError)?;
    let build = request.build;
    let build_id = build.id;
    let payload = json!({"build_id": build_id});
    state
        .jobs
        .enqueue(
            &mut transaction,
            &context,
            DESIGN_BUILD_WORKFLOW,
            &payload,
            None,
            Some(&format!("design-build:{build_id}")),
        )
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(build)))
}

async fn publish_design_change(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
) -> Result<Json<DesignChange>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Publish, Action::Write),
        "DesignChange",
        change_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let change = state
        .design
        .publish(&mut transaction, &context, change_id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(change))
}

async fn rollback_design_change(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
) -> Result<Json<DesignChange>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Publish, Action::Write),
        "DesignChange",
        change_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let change = state
        .design
        .rollback(&mut transaction, &context, change_id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(change))
}

async fn preview_design_asset(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path((build_id, path)): Path<(DesignBuildId, String)>,
) -> Result<Response, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let artifact = state
        .design
        .preview_artifact(&mut transaction, &context, build_id, &path)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    let bytes = state
        .file_store
        .get(&context, &artifact.storage_key)
        .await
        .map_err(HttpError)?;
    asset_response(artifact.mime, bytes)
}

async fn public_design_asset(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(path): Path<String>,
) -> Result<Response, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let artifact = state
        .design
        .live_artifact(&mut transaction, &context, &path)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    let bytes = state
        .file_store
        .get(&context, &artifact.storage_key)
        .await
        .map_err(HttpError)?;
    asset_response(artifact.mime, bytes)
}

fn asset_response(mime: String, bytes: Vec<u8>) -> Result<Response, HttpError> {
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, mime)
        .header(CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(Body::from(bytes))
        .map_err(|_| HttpError(MaviError::Internal))
}
