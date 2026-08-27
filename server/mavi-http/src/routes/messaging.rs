#![allow(clippy::wildcard_imports)]

use super::super::*;

pub(super) fn mail_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/api/v1/mail/templates",
            get(list_mail_templates).post(create_mail_template),
        )
        .route(
            "/api/v1/mail/templates/{id}",
            get(read_mail_template)
                .patch(update_mail_template)
                .delete(delete_mail_template),
        )
        .route(
            "/api/v1/mail/templates/{id}/preview",
            post(preview_mail_template),
        )
        .route(
            "/api/v1/mail/lists",
            get(list_mail_lists).post(create_mail_list),
        )
        .route(
            "/api/v1/mail/lists/{id}",
            get(read_mail_list)
                .patch(update_mail_list)
                .delete(delete_mail_list),
        )
        .route(
            "/api/v1/mail/lists/{id}/readers",
            get(list_mail_readers).post(add_mail_reader),
        )
        .route(
            "/api/v1/mail/lists/{id}/deliveries",
            post(send_mail_campaign),
        )
        .route("/api/v1/mail/readers/{id}", delete(delete_mail_reader))
        .route(
            "/api/v1/mail/deliveries",
            get(list_mail_deliveries).post(enqueue_mail_delivery),
        )
        .route("/api/v1/mail/deliveries/{id}", get(read_mail_delivery))
        .route(
            "/api/v1/mail/deliveries/{id}/retry",
            post(retry_mail_delivery),
        )
        .route(
            "/public/v1/mail/unsubscribe/{token}",
            post(public_mail_unsubscribe),
        )
        .route(MAIL_PROVIDER_EVENTS_PATH, post(receive_mail_provider_event))
}

async fn list_mail_templates(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<MailTemplateListFilter>,
) -> Result<Json<Page<MailTemplate>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::View),
        "MailTemplate",
        "mail_templates",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let templates = state
        .mail
        .list_templates(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(templates))
}

async fn create_mail_template(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateMailTemplate>,
) -> Result<(StatusCode, Json<MailTemplate>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::Write),
        "MailTemplate",
        "mail_templates",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let template = state
        .mail
        .create_template(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(template)))
}

async fn read_mail_template(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailTemplateId>,
) -> Result<Json<MailTemplate>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::View),
        "MailTemplate",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let template = state
        .mail
        .get_template(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(template))
}

async fn update_mail_template(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailTemplateId>,
    Json(input): Json<UpdateMailTemplate>,
) -> Result<Json<MailTemplate>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::Write),
        "MailTemplate",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let template = state
        .mail
        .update_template(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(template))
}

async fn delete_mail_template(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailTemplateId>,
) -> Result<StatusCode, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::Delete),
        "MailTemplate",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .mail
        .delete_template(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn preview_mail_template(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailTemplateId>,
    Json(input): Json<MailTemplatePreview>,
) -> Result<Json<RenderedMail>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::View),
        "MailTemplate",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let rendered = state
        .mail
        .preview_template(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(rendered))
}

async fn list_mail_lists(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<MailListListFilter>,
) -> Result<Json<Page<MailList>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::View),
        "MailList",
        "mail_lists",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let lists = state
        .mail
        .list_lists(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(lists))
}

async fn create_mail_list(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateMailList>,
) -> Result<(StatusCode, Json<MailList>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::Write),
        "MailList",
        "mail_lists",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let list = state
        .mail
        .create_list(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(list)))
}

async fn read_mail_list(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailListId>,
) -> Result<Json<MailList>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::View),
        "MailList",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let list = state
        .mail
        .get_list(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(list))
}

async fn update_mail_list(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailListId>,
    Json(input): Json<UpdateMailList>,
) -> Result<Json<MailList>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::Write),
        "MailList",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let list = state
        .mail
        .update_list(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(list))
}

async fn delete_mail_list(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailListId>,
) -> Result<StatusCode, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::Delete),
        "MailList",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .mail
        .delete_list(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_mail_readers(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(list_id): Path<MailListId>,
    Query(filter): Query<ReaderListFilter>,
) -> Result<Json<Page<MailReader>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::View),
        "MailList",
        list_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let readers = state
        .mail
        .list_readers(&mut transaction, &context, list_id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(readers))
}

async fn add_mail_reader(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(list_id): Path<MailListId>,
    Json(input): Json<AddReader>,
) -> Result<(StatusCode, Json<MailReaderCreated>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::Write),
        "MailList",
        list_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let reader = state
        .mail
        .add_reader(&mut transaction, &context, list_id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(reader)))
}

async fn delete_mail_reader(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailReaderId>,
) -> Result<StatusCode, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::Delete),
        "MailReader",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .mail
        .delete_reader(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn public_mail_unsubscribe(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(token): Path<String>,
) -> Result<Json<UnsubscribeReceipt>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let receipt = state
        .mail
        .unsubscribe(&mut transaction, &context, &token)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(receipt))
}

async fn receive_mail_provider_event(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<ReceiveMailProviderEvent>,
) -> Result<Json<MailProviderEventReceipt>, HttpError> {
    if !matches!(
        &context.caller,
        Caller::System { worker } if worker == "mail-webhook"
    ) {
        return Err(HttpError(MaviError::Unauthenticated));
    }
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let receipt = state
        .mail
        .receive_provider_event(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(receipt))
}

async fn list_mail_deliveries(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<DeliveryListFilter>,
) -> Result<Json<Page<MailDelivery>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::View),
        "MailDelivery",
        "mail_deliveries",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let deliveries = state
        .mail
        .list_deliveries(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(deliveries))
}

async fn enqueue_mail_delivery(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<EnqueueDelivery>,
) -> Result<(StatusCode, Json<MailDelivery>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::Write),
        "MailDelivery",
        "mail_deliveries",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let delivery = state
        .mail
        .enqueue_delivery(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::ACCEPTED, Json(delivery)))
}

async fn read_mail_delivery(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailDeliveryId>,
) -> Result<Json<MailDelivery>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::View),
        "MailDelivery",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let delivery = state
        .mail
        .get_delivery(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(delivery))
}

async fn retry_mail_delivery(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailDeliveryId>,
    Json(_input): Json<RetryDelivery>,
) -> Result<(StatusCode, Json<MailDelivery>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::Write),
        "MailDelivery",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let delivery = state
        .mail
        .retry_delivery(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::ACCEPTED, Json(delivery)))
}

async fn send_mail_campaign(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(list_id): Path<MailListId>,
    Json(input): Json<SendCampaign>,
) -> Result<(StatusCode, Json<SendCount>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Mail, Action::Write),
        "MailList",
        list_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let count = state
        .mail
        .send_campaign(
            &mut transaction,
            &context,
            list_id,
            &input,
            state.sealer.as_ref(),
        )
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::ACCEPTED, Json(count)))
}
