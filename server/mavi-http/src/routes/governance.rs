#![allow(clippy::wildcard_imports)]

use super::super::*;

pub(super) fn audit_trash_routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/audit", get(list_audit))
        .route("/api/v1/audit/export", get(export_audit))
        .route("/api/v1/audit/{id}", get(read_audit))
        .route("/api/v1/trash", get(list_trash))
        .route("/api/v1/trash/{kind}/{id}/restore", post(restore_trash))
        .route(
            "/api/v1/trash/{kind}/{id}",
            delete(permanently_delete_trash),
        )
}

pub(super) fn portable_routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/portable/export", get(export_portable))
        .route("/api/v1/portable/import", post(import_portable))
}

async fn export_portable(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
) -> Result<Json<PortableBundle>, HttpError> {
    require_portable_grant(
        &state,
        &context,
        Action::View,
        "PortableBundle",
        "site_export",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let bundle = state
        .portable
        .export(&mut transaction, &context)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(bundle))
}

async fn import_portable(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<PortableImportRequest>,
) -> Result<Json<ImportReceipt>, HttpError> {
    require_portable_grant(
        &state,
        &context,
        Action::Write,
        "PortableBundle",
        "site_import",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let receipt = state
        .portable
        .import(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(receipt))
}
async fn list_audit(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<AuditListFilter>,
) -> Result<Json<Page<AuditEvent>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Audit, Action::View),
        "AuditEvent",
        "audit_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let events = state
        .audit
        .list(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(events))
}

async fn export_audit(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<AuditExportFilter>,
) -> Result<Json<AuditExport>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Audit, Action::View),
        "AuditExport",
        "audit_export",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let export = state
        .audit
        .export(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    state
        .audit
        .record(
            &mut transaction,
            &context,
            &AuditEntry {
                action: "audit.events.exported".to_owned(),
                resource_type: "AuditExport".to_owned(),
                resource_id: None,
                payload: json!({
                    "format": &export.format,
                    "version": export.version,
                    "count": export.items.len(),
                    "truncated": export.truncated,
                }),
            },
        )
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(export))
}

async fn read_audit(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<AuditEventId>,
) -> Result<Json<AuditEvent>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Audit, Action::View),
        "AuditEvent",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let event = state
        .audit
        .get(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(event))
}

async fn list_trash(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<TrashListFilter>,
) -> Result<Json<Page<TrashItem>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Trash, Action::View),
        "TrashItem",
        "trash_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let items = state
        .trash
        .list(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(items))
}

async fn restore_trash(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path((kind, id)): Path<(String, Uuid)>,
) -> Result<StatusCode, HttpError> {
    let kind = TrashKind::parse(&kind).map_err(HttpError)?;
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Trash, Action::Write),
        kind.resource_type(),
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .trash
        .restore(&mut transaction, &context, kind, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn permanently_delete_trash(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path((kind, id)): Path<(String, Uuid)>,
) -> Result<StatusCode, HttpError> {
    let kind = TrashKind::parse(&kind).map_err(HttpError)?;
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Trash, Action::Delete),
        kind.resource_type(),
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let deletion = state
        .trash
        .permanently_delete(&mut transaction, &context, kind, id)
        .await
        .map_err(HttpError)?;
    if let (Some(file_id), Some(storage_key)) = (deletion.file_id, deletion.file_storage_key) {
        state
            .media
            .enqueue_cleanup_job(
                &mut transaction,
                &context,
                &state.jobs,
                FileId::from_uuid(file_id),
                &storage_key,
            )
            .await
            .map_err(HttpError)?;
    }
    transaction.commit().await.map_err(HttpError)?;

    Ok(StatusCode::NO_CONTENT)
}
