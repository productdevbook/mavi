#![allow(clippy::wildcard_imports)]

use super::super::*;

pub(super) fn plugin_routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/plugins", get(list_plugins))
        .route("/api/v1/plugins/{id}/enable", post(enable_plugin))
        .route("/api/v1/plugins/{id}/disable", post(disable_plugin))
}

async fn list_plugins(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
) -> Result<Json<Vec<PluginRecord>>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let records = state
        .plugins
        .list_for_context(&mut transaction, &context, &state.authorization)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(records))
}

async fn enable_plugin(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<String>,
) -> Result<Json<PluginRecord>, HttpError> {
    Ok(Json(
        change_plugin(&state, &context, &id, true)
            .await
            .map_err(HttpError)?,
    ))
}

async fn disable_plugin(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<String>,
) -> Result<Json<PluginRecord>, HttpError> {
    Ok(Json(
        change_plugin(&state, &context, &id, false)
            .await
            .map_err(HttpError)?,
    ))
}

async fn change_plugin(
    state: &HttpState,
    context: &SiteContext,
    raw_id: &str,
    enabled: bool,
) -> Result<PluginRecord, MaviError> {
    let id = raw_id
        .parse::<PluginId>()
        .map_err(|()| MaviError::validation("plugin_id_invalid"))?;
    let mut transaction = state.runtime.begin(context).await?;
    let record = state
        .plugins
        .set_enabled_for_context(&mut transaction, context, &state.authorization, id, enabled)
        .await?;
    state
        .audit
        .record(
            &mut transaction,
            context,
            &AuditEntry {
                action: format!("plugins.{}", if enabled { "enabled" } else { "disabled" }),
                resource_type: "Plugin".to_owned(),
                resource_id: Some(context.site_id.into_uuid()),
                payload: json!({"plugin_id": id, "enabled": enabled}),
            },
        )
        .await?;
    transaction.commit().await?;
    state.plugins.invalidate();
    Ok(record)
}

pub(super) async fn runtime_manifest(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
) -> Result<Json<RuntimeManifest>, HttpError> {
    let active = active_plugins(&state, &context).await.map_err(HttpError)?;
    let api_hash = api()
        .for_plugins(&active)
        .fingerprint()
        .map_err(|_| HttpError(MaviError::Internal))?;
    Ok(Json(state.runtime.manifest(api_hash, active)))
}

pub(super) fn identity_routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/setup", get(setup_status).post(setup_initialize))
        .route("/api/v1/auth/sessions", post(create_session))
        .route("/api/v1/auth/password-resets", post(request_password_reset))
        .route(
            "/api/v1/auth/password-resets/redeem",
            post(redeem_password_reset),
        )
        .route(
            "/api/v1/auth/email-verifications",
            post(request_email_verification),
        )
        .route(
            "/api/v1/auth/email-verifications/redeem",
            post(redeem_email_verification),
        )
        .route(
            "/api/v1/auth/sessions/current",
            get(current_session).delete(revoke_session),
        )
        .route(
            "/api/v1/auth/api-keys",
            get(list_api_keys).post(create_api_key),
        )
        .route("/api/v1/auth/api-keys/{id}", delete(revoke_api_key))
        .route("/api/v1/people", get(list_people).post(create_person))
        .route(
            "/api/v1/people/{id}/status",
            axum::routing::patch(update_person_status),
        )
        .route("/api/v1/people/{id}/roles", put(replace_person_roles))
        .route("/api/v1/roles", get(list_roles).post(create_role))
        .route("/api/v1/roles/{id}", delete(delete_role))
        .route("/api/v1/roles/{id}/grants", put(replace_role_grants))
}

async fn setup_status(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
) -> Result<Json<SetupStatus>, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let status = state
        .identity
        .status(&mut transaction, &context)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(status))
}

async fn setup_initialize(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<SetupInput>,
) -> Result<(StatusCode, Json<Person>), HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let person = state
        .identity
        .initialize(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    state
        .settings
        .initialize(&mut transaction, &context, &input.site_name)
        .await
        .map_err(HttpError)?;
    state
        .content
        .initialize(&mut transaction, &context)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(person)))
}

async fn create_session(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<LoginInput>,
) -> Result<(StatusCode, Json<SessionCreated>), HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let result = state
        .identity
        .create_session(&mut transaction, &context, &input, Utc::now())
        .await;
    match result {
        Ok(session) => {
            transaction.commit().await.map_err(HttpError)?;
            Ok((StatusCode::CREATED, Json(session)))
        }
        Err(error) => {
            // Invalid credentials and a correct-but-unverified password both
            // write security receipts. Commit those deliberate negative
            // outcomes; other failures remain rolled back by dropping the tx.
            if matches!(
                &error,
                MaviError::Unauthenticated | MaviError::Conflict { .. }
            ) {
                transaction.commit().await.map_err(HttpError)?;
            }
            Err(HttpError(error))
        }
    }
}

async fn request_password_reset(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<PasswordResetRequestInput>,
) -> Result<(StatusCode, Json<PasswordResetRequested>), HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let notification = state
        .identity
        .request_password_reset(&mut transaction, &context, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    if let Some(notification) = notification {
        let idempotency_key = format!("password-reset:{}", notification.id);
        let body = format!(
            "Use this one-time Mavi password reset token within one hour:\n\n{}\n\nThis token expires at {}. If you did not request a password reset, you can ignore this message.",
            notification.token,
            notification.expires_at.to_rfc3339(),
        );
        state
            .mail
            .enqueue_protected_transactional_message(
                &mut transaction,
                &context,
                MailMessage {
                    recipient: notification.recipient.as_str().to_owned(),
                    subject: "Reset your Mavi password".to_owned(),
                    body,
                    content_type: MailContentType::Plain,
                    unsubscribe_url: None,
                },
                Some(&idempotency_key),
                state.sealer.as_ref(),
            )
            .await
            .map_err(HttpError)?;
    }
    transaction.commit().await.map_err(HttpError)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(PasswordResetRequested { accepted: true }),
    ))
}

async fn redeem_password_reset(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<PasswordResetRedeemInput>,
) -> Result<StatusCode, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .identity
        .redeem_password_reset(&mut transaction, &context, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn request_email_verification(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<EmailVerificationRequestInput>,
) -> Result<(StatusCode, Json<EmailVerificationRequested>), HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let notification = state
        .identity
        .request_email_verification(&mut transaction, &context, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    if let Some(notification) = notification {
        let idempotency_key = format!("email-verification:{}", notification.id);
        let body = format!(
            "Use this one-time Mavi email verification token before its expiry timestamp:\n\n{}\n\nThis token expires at {}. If you did not request email verification, you can ignore this message.",
            notification.token,
            notification.expires_at.to_rfc3339(),
        );
        state
            .mail
            .enqueue_protected_transactional_message(
                &mut transaction,
                &context,
                MailMessage {
                    recipient: notification.recipient.as_str().to_owned(),
                    subject: "Verify your Mavi email".to_owned(),
                    body,
                    content_type: MailContentType::Plain,
                    unsubscribe_url: None,
                },
                Some(&idempotency_key),
                state.sealer.as_ref(),
            )
            .await
            .map_err(HttpError)?;
    }
    transaction.commit().await.map_err(HttpError)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(EmailVerificationRequested { accepted: true }),
    ))
}

async fn redeem_email_verification(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<EmailVerificationRedeemInput>,
) -> Result<StatusCode, HttpError> {
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .identity
        .redeem_email_verification(&mut transaction, &context, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn revoke_session(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
) -> Result<StatusCode, HttpError> {
    if !matches!(context.caller, Caller::Account { .. }) {
        return Err(HttpError(MaviError::Unauthenticated));
    }
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .identity
        .revoke_current(&mut transaction, &context, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn current_session(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
) -> Result<Json<CurrentSession>, HttpError> {
    if !matches!(context.caller, Caller::Account { .. }) {
        return Err(HttpError(MaviError::Unauthenticated));
    }
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let session = state
        .identity
        .current_session(&mut transaction, &context)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(session))
}

async fn list_api_keys(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<ApiKeyListFilter>,
) -> Result<Json<Page<ApiKeyRecord>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::People, Action::View),
        "ApiKey",
        "api_key_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .identity
        .list_api_keys(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn create_api_key(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateApiKey>,
) -> Result<(StatusCode, Json<ApiKeyCreated>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::People, Action::Write),
        "ApiKey",
        "api_key_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let key = state
        .identity
        .create_api_key(&mut transaction, &context, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(key)))
}

async fn revoke_api_key(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<mavi_core::ApiKeyId>,
) -> Result<StatusCode, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::People, Action::Delete),
        "ApiKey",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .identity
        .revoke_api_key(&mut transaction, &context, id, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_people(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<PeopleListFilter>,
) -> Result<Json<Page<PersonRecord>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::People, Action::View),
        "Person",
        "people_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .identity
        .list_people(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn create_person(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreatePerson>,
) -> Result<(StatusCode, Json<PersonRecord>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::People, Action::Write),
        "Person",
        "people_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let person = state
        .identity
        .create_person(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(person)))
}

async fn update_person_status(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<PersonId>,
    Json(input): Json<UpdatePersonStatus>,
) -> Result<Json<PersonRecord>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::People, Action::Write),
        "Person",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let person = state
        .identity
        .update_person_status(&mut transaction, &context, id, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(person))
}

async fn replace_person_roles(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<PersonId>,
    Json(input): Json<ReplacePersonRoles>,
) -> Result<Json<PersonRecord>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::People, Action::Write),
        "Person",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let person = state
        .identity
        .replace_person_roles(&mut transaction, &context, id, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(person))
}

async fn list_roles(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<RoleListFilter>,
) -> Result<Json<Page<Role>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::People, Action::View),
        "Role",
        "roles_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .identity
        .list_roles(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn create_role(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateRole>,
) -> Result<(StatusCode, Json<Role>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::People, Action::Write),
        "Role",
        "roles_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let role = state
        .identity
        .create_role(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(role)))
}

async fn replace_role_grants(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<RoleId>,
    Json(input): Json<ReplaceRoleGrants>,
) -> Result<Json<Role>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::People, Action::Write),
        "Role",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let role = state
        .identity
        .replace_role_grants(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(role))
}

async fn delete_role(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<RoleId>,
) -> Result<StatusCode, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::People, Action::Delete),
        "Role",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .identity
        .delete_role(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) fn settings_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/api/v1/settings",
            get(read_settings).patch(update_settings),
        )
        .route(
            "/api/v1/languages",
            get(list_languages).post(create_language),
        )
        .route(
            "/api/v1/languages/{tag}",
            axum::routing::patch(update_language).delete(delete_language),
        )
}

async fn read_settings(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
) -> Result<Json<SiteSettings>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Settings, Action::View),
        "SiteSettings",
        context.site_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let settings = state
        .settings
        .get_settings(&mut transaction, &context)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(settings))
}

async fn update_settings(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<UpdateSiteSettings>,
) -> Result<Json<SiteSettings>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Settings, Action::Write),
        "SiteSettings",
        context.site_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let settings = state
        .settings
        .update_settings(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(settings))
}

async fn list_languages(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<LanguageListFilter>,
) -> Result<Json<Page<Language>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Settings, Action::View),
        "Language",
        "language_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let languages = state
        .settings
        .list_languages(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(languages))
}

async fn create_language(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateLanguage>,
) -> Result<(StatusCode, Json<Language>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Settings, Action::Write),
        "Language",
        "language_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let language = state
        .settings
        .create_language(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(language)))
}

async fn update_language(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(tag): Path<String>,
    Json(input): Json<UpdateLanguage>,
) -> Result<Json<Language>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Settings, Action::Write),
        "Language",
        tag.clone(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let language = state
        .settings
        .update_language(&mut transaction, &context, &tag, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(language))
}

async fn delete_language(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(tag): Path<String>,
) -> Result<StatusCode, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Settings, Action::Delete),
        "Language",
        tag.clone(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .settings
        .delete_language(&mut transaction, &context, &tag)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) fn credentials_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/api/v1/credentials",
            get(list_credentials).post(create_credential),
        )
        .route(
            "/api/v1/credentials/{id}",
            put(rotate_credential).delete(revoke_credential),
        )
}

async fn list_credentials(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<CredentialListFilter>,
) -> Result<Json<Page<Credential>>, HttpError> {
    require_credentials_grant(&state, &context, Action::View, "credentials_collection")?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let credentials = state
        .credentials
        .list(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(credentials))
}

async fn create_credential(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateCredential>,
) -> Result<(StatusCode, Json<Credential>), HttpError> {
    require_credentials_grant(&state, &context, Action::Write, "credentials_collection")?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let credential = state
        .credentials
        .create(&mut transaction, &context, state.sealer.as_ref(), &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(credential)))
}

async fn rotate_credential(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<CredentialId>,
    Json(input): Json<RotateCredential>,
) -> Result<Json<Credential>, HttpError> {
    require_credentials_grant(&state, &context, Action::Write, id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let credential = state
        .credentials
        .rotate(
            &mut transaction,
            &context,
            state.sealer.as_ref(),
            id,
            &input,
        )
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(credential))
}

async fn revoke_credential(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<CredentialId>,
) -> Result<StatusCode, HttpError> {
    require_credentials_grant(&state, &context, Action::Delete, id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .credentials
        .revoke(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) fn feedback_routes() -> Router<HttpState> {
    Router::new().route(
        "/api/v1/feedback/reports",
        get(list_feedback_reports).post(create_feedback_report),
    )
}

async fn create_feedback_report(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateReport>,
) -> Result<(StatusCode, Json<Report>), HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Feedback, Action::Write),
        "FeedbackReport",
        "feedback_reports",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let report = state
        .feedback
        .create(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(report)))
}

async fn list_feedback_reports(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<ReportListFilter>,
) -> Result<Json<Page<Report>>, HttpError> {
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Feedback, Action::View),
        "FeedbackReport",
        "feedback_reports",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let reports = state
        .feedback
        .list(&mut transaction, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(reports))
}
