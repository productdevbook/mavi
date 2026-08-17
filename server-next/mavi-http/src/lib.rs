//! HTTP boundary for the clean Mavi implementation.
//!
//! This crate admits a request into a [`SiteContext`] before a handler runs.
//! Handlers receive the context as an extension and cannot silently resolve a
//! different site halfway through an operation.

use axum::{
    Extension, Router,
    extract::{Json, Path, Query, State},
    http::{HeaderValue, Request, StatusCode, header::AUTHORIZATION},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use chrono::Utc;
use mavi_authz::CedarAuthorizer;
use mavi_content::{
    Content, ContentListFilter, ContentRevision, ContentRevisionListFilter, ContentService,
    ContentType, ContentTypeListFilter, CreateContent, DeclareContentType, PublicationInput,
    ScheduleContent, UpdateContent,
};
use mavi_contract::Api;
use mavi_core::{
    Action, Caller, Capability, ContentId, ErrorCode, Grant, MaviError, Page, PersonId, RequestId,
    RoleId, SiteContext,
};
use mavi_identity::{
    ApiKeyCreated, CreateApiKey, CreatePerson, CreateRole, IdentityService, LoginInput,
    PeopleListFilter, Person, PersonRecord, ReplaceRoleGrants, Role, RoleListFilter,
    SessionCreated, SetupInput, SetupStatus, UpdatePersonStatus,
};
use mavi_runtime::{Runtime, SiteResolver};
use mavi_settings::{
    CreateLanguage, Language, LanguageListFilter, SettingsService, SiteSettings, UpdateLanguage,
    UpdateSiteSettings,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Clone, Debug, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Clone, Debug, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

impl From<MaviError> for ErrorEnvelope {
    fn from(error: MaviError) -> Self {
        let error_code_value = error.code();
        let (code, field) = match error {
            MaviError::Validation { code, field } => (code, field),
            MaviError::Conflict { code } => (code, None),
            _ => (error_code(error_code_value), None),
        };

        Self {
            error: ErrorBody {
                code,
                message: error_message(error_code_value),
                field,
            },
        }
    }
}

#[derive(Debug)]
pub struct HttpError(pub MaviError);

impl From<MaviError> for HttpError {
    fn from(error: MaviError) -> Self {
        Self(error)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let status = status_code(self.0.code());
        (status, axum::Json(ErrorEnvelope::from(self.0))).into_response()
    }
}

/// Returns the complete site API catalog used by documentation and clients.
///
/// Each domain owns its endpoint declarations; the HTTP composition root is
/// the only place that combines them into the application contract.
#[must_use]
pub fn api() -> Api {
    let mut api = mavi_identity::api();
    api.extend(mavi_content::api());
    api.extend(mavi_settings::api());
    api
}

async fn openapi_document() -> Result<Json<Value>, HttpError> {
    api()
        .openapi("Mavi", "0.1.0")
        .map(Json)
        .map_err(|_| HttpError(MaviError::Internal))
}

/// Builds the shared router and admits every request into a site context.
pub fn router<R>(runtime: Runtime<R>) -> Result<Router, MaviError>
where
    R: SiteResolver,
{
    let state = HttpState {
        runtime: runtime.clone(),
        identity: IdentityService,
        content: ContentService,
        settings: SettingsService,
        authorizer: CedarAuthorizer::new()?,
    };
    Ok(runtime
        .router::<HttpState<R>>()
        .route("/openapi.json", get(openapi_document))
        .route(
            "/api/v1/setup",
            get(setup_status::<R>).post(setup_initialize::<R>),
        )
        .route("/api/v1/auth/sessions", post(create_session::<R>))
        .route("/api/v1/auth/sessions/current", delete(revoke_session::<R>))
        .route("/api/v1/auth/api-keys", post(create_api_key::<R>))
        .route("/api/v1/auth/api-keys/{id}", delete(revoke_api_key::<R>))
        .route(
            "/api/v1/people",
            get(list_people::<R>).post(create_person::<R>),
        )
        .route(
            "/api/v1/people/{id}/status",
            axum::routing::patch(update_person_status::<R>),
        )
        .route("/api/v1/roles", get(list_roles::<R>).post(create_role::<R>))
        .route("/api/v1/roles/{id}/grants", put(replace_role_grants::<R>))
        .route(
            "/api/v1/settings",
            get(read_settings::<R>).patch(update_settings::<R>),
        )
        .route(
            "/api/v1/languages",
            get(list_languages::<R>).post(create_language::<R>),
        )
        .route(
            "/api/v1/languages/{tag}",
            axum::routing::patch(update_language::<R>).delete(delete_language::<R>),
        )
        .route("/api/v1/content-types", get(list_content_types::<R>))
        .route(
            "/api/v1/content-types/{kind}",
            put(upsert_content_type::<R>).delete(delete_content_type::<R>),
        )
        .route(
            "/api/v1/content/{id}/revisions",
            get(list_content_revisions::<R>),
        )
        .route(
            "/api/v1/content/{id}/revisions/{revision}",
            get(read_content_revision::<R>),
        )
        .route(
            "/api/v1/content/{id}",
            get(read_content::<R>)
                .patch(update_content::<R>)
                .delete(trash_content::<R>),
        )
        .route(
            "/api/v1/content",
            get(list_content::<R>).post(create_content::<R>),
        )
        .route("/api/v1/content/{id}/publish", post(publish_content::<R>))
        .route("/api/v1/content/{id}/schedule", post(schedule_content::<R>))
        .route("/api/v1/content/{id}/archive", post(archive_content::<R>))
        .route("/api/v1/content/{id}/restore", post(restore_content::<R>))
        .route("/public/v1/content/{slug}", get(public_content::<R>))
        .layer(middleware::from_fn_with_state(
            runtime.clone(),
            authenticate::<R>,
        ))
        .layer(middleware::from_fn_with_state(runtime, admit::<R>))
        .with_state(state))
}

struct HttpState<R> {
    runtime: Runtime<R>,
    identity: IdentityService,
    content: ContentService,
    settings: SettingsService,
    authorizer: CedarAuthorizer,
}

impl<R> Clone for HttpState<R> {
    fn clone(&self) -> Self {
        Self {
            runtime: self.runtime.clone(),
            identity: self.identity,
            content: self.content,
            settings: self.settings,
            authorizer: self.authorizer.clone(),
        }
    }
}

/// Returns the context inserted by the admission layer.
pub fn context(request: &Request<axum::body::Body>) -> Result<&SiteContext, MaviError> {
    request
        .extensions()
        .get::<SiteContext>()
        .ok_or(MaviError::Internal)
}

async fn admit<R>(
    State(runtime): State<Runtime<R>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Response
where
    R: SiteResolver,
{
    let request_id = RequestId::new();
    let request_id_header = HeaderValue::from_str(&request_id.to_string())
        .expect("UUID request IDs are always valid header values");

    let response = match runtime.context(request.headers().clone(), request_id).await {
        Ok(site_context) => {
            request.extensions_mut().insert(site_context);
            next.run(request).await
        }
        Err(error) => HttpError(error).into_response(),
    };

    let mut response = response;
    response
        .headers_mut()
        .insert(REQUEST_ID_HEADER, request_id_header);
    response
}

async fn authenticate<R>(
    State(runtime): State<Runtime<R>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Response
where
    R: SiteResolver,
{
    let token = match authorization_token(&request) {
        Ok(Some(token)) => token,
        Ok(None) => return next.run(request).await,
        Err(error) => return HttpError(error).into_response(),
    };
    let Some(public_context) = request.extensions().get::<SiteContext>().cloned() else {
        return HttpError(MaviError::Internal).into_response();
    };

    let mut transaction = match runtime.begin(&public_context).await {
        Ok(transaction) => transaction,
        Err(error) => return HttpError(error).into_response(),
    };
    let caller = match IdentityService
        .authenticate_bearer(&mut transaction, &public_context, token, Utc::now())
        .await
    {
        Ok(caller) => caller,
        Err(error) => return HttpError(error).into_response(),
    };
    if let Err(error) = transaction.commit().await {
        return HttpError(error).into_response();
    }

    request.extensions_mut().insert(SiteContext::with_caller(
        public_context.site_id,
        caller,
        public_context.request_id,
    ));
    next.run(request).await
}

fn authorization_token(request: &Request<axum::body::Body>) -> Result<Option<&str>, MaviError> {
    let Some(value) = request.headers().get(AUTHORIZATION) else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| MaviError::Unauthenticated)?;
    let token = value
        .strip_prefix("Bearer ")
        .filter(|token| !token.is_empty())
        .ok_or(MaviError::Unauthenticated)?;
    Ok(Some(token))
}

async fn setup_status<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
) -> Result<axum::Json<SetupStatus>, HttpError>
where
    R: SiteResolver,
{
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let status = state
        .identity
        .status(&mut transaction, &context)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(status))
}

async fn setup_initialize<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<SetupInput>,
) -> Result<(StatusCode, Json<Person>), HttpError>
where
    R: SiteResolver,
{
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

async fn create_session<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<LoginInput>,
) -> Result<(StatusCode, Json<SessionCreated>), HttpError>
where
    R: SiteResolver,
{
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let session = state
        .identity
        .create_session(&mut transaction, &context, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(session)))
}

async fn revoke_session<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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

async fn create_api_key<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateApiKey>,
) -> Result<(StatusCode, Json<ApiKeyCreated>), HttpError>
where
    R: SiteResolver,
{
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

async fn revoke_api_key<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<mavi_core::ApiKeyId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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

async fn list_people<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<PeopleListFilter>,
) -> Result<Json<Page<PersonRecord>>, HttpError>
where
    R: SiteResolver,
{
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

async fn create_person<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreatePerson>,
) -> Result<(StatusCode, Json<PersonRecord>), HttpError>
where
    R: SiteResolver,
{
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

async fn update_person_status<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<PersonId>,
    Json(input): Json<UpdatePersonStatus>,
) -> Result<Json<PersonRecord>, HttpError>
where
    R: SiteResolver,
{
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

async fn list_roles<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<RoleListFilter>,
) -> Result<Json<Page<Role>>, HttpError>
where
    R: SiteResolver,
{
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

async fn create_role<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateRole>,
) -> Result<(StatusCode, Json<Role>), HttpError>
where
    R: SiteResolver,
{
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

async fn replace_role_grants<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<RoleId>,
    Json(input): Json<ReplaceRoleGrants>,
) -> Result<Json<Role>, HttpError>
where
    R: SiteResolver,
{
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

async fn read_settings<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
) -> Result<Json<SiteSettings>, HttpError>
where
    R: SiteResolver,
{
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

async fn update_settings<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<UpdateSiteSettings>,
) -> Result<Json<SiteSettings>, HttpError>
where
    R: SiteResolver,
{
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

async fn list_languages<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<LanguageListFilter>,
) -> Result<Json<Page<Language>>, HttpError>
where
    R: SiteResolver,
{
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

async fn create_language<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateLanguage>,
) -> Result<(StatusCode, Json<Language>), HttpError>
where
    R: SiteResolver,
{
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

async fn update_language<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(tag): Path<String>,
    Json(input): Json<UpdateLanguage>,
) -> Result<Json<Language>, HttpError>
where
    R: SiteResolver,
{
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

async fn delete_language<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(tag): Path<String>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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

async fn list_content_types<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<ContentTypeListFilter>,
) -> Result<Json<Page<ContentType>>, HttpError>
where
    R: SiteResolver,
{
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

async fn upsert_content_type<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(kind): Path<String>,
    Json(input): Json<DeclareContentType>,
) -> Result<Json<ContentType>, HttpError>
where
    R: SiteResolver,
{
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

async fn delete_content_type<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(kind): Path<String>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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

async fn list_content_revisions<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
    Query(filter): Query<ContentRevisionListFilter>,
) -> Result<Json<Page<ContentRevision>>, HttpError>
where
    R: SiteResolver,
{
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

async fn read_content_revision<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path((id, revision)): Path<(ContentId, u32)>,
) -> Result<Json<ContentRevision>, HttpError>
where
    R: SiteResolver,
{
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

async fn read_content<R>(
    State(state): State<HttpState<R>>,
    Extension(site_context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
) -> Result<Json<Content>, HttpError>
where
    R: SiteResolver,
{
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

async fn list_content<R>(
    State(state): State<HttpState<R>>,
    Extension(site_context): Extension<SiteContext>,
    Query(filter): Query<ContentListFilter>,
) -> Result<Json<Page<Content>>, HttpError>
where
    R: SiteResolver,
{
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

async fn create_content<R>(
    State(state): State<HttpState<R>>,
    Extension(site_context): Extension<SiteContext>,
    Json(input): Json<CreateContent>,
) -> Result<(StatusCode, Json<Content>), HttpError>
where
    R: SiteResolver,
{
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
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(entry)))
}

async fn update_content<R>(
    State(state): State<HttpState<R>>,
    Extension(site_context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
    Json(input): Json<UpdateContent>,
) -> Result<Json<Content>, HttpError>
where
    R: SiteResolver,
{
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
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(entry))
}

async fn publish_content<R>(
    State(state): State<HttpState<R>>,
    Extension(site_context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
) -> Result<Json<Content>, HttpError>
where
    R: SiteResolver,
{
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

async fn schedule_content<R>(
    State(state): State<HttpState<R>>,
    Extension(site_context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
    Json(input): Json<ScheduleContent>,
) -> Result<Json<Content>, HttpError>
where
    R: SiteResolver,
{
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
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(entry))
}

async fn archive_content<R>(
    State(state): State<HttpState<R>>,
    Extension(site_context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
) -> Result<Json<Content>, HttpError>
where
    R: SiteResolver,
{
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

async fn trash_content<R>(
    State(state): State<HttpState<R>>,
    Extension(site_context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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

async fn restore_content<R>(
    State(state): State<HttpState<R>>,
    Extension(site_context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
) -> Result<Json<Content>, HttpError>
where
    R: SiteResolver,
{
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
    #[serde(default = "default_language")]
    language: String,
}

fn default_language() -> String {
    "en".to_owned()
}

async fn public_content<R>(
    State(state): State<HttpState<R>>,
    Extension(site_context): Extension<SiteContext>,
    Path(slug): Path<String>,
    Query(query): Query<PublicContentQuery>,
) -> Result<Json<Content>, HttpError>
where
    R: SiteResolver,
{
    let mut transaction = state
        .runtime
        .begin(&site_context)
        .await
        .map_err(HttpError)?;
    let entry = state
        .content
        .public_get(&mut transaction, &site_context, &query.language, &slug)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(entry))
}

fn require_grant<R>(
    state: &HttpState<R>,
    context: &SiteContext,
    grant: Grant,
    resource_id: impl Into<String>,
) -> Result<(), HttpError>
where
    R: SiteResolver,
{
    require_grant_for(state, context, grant, "Content", resource_id)
}

fn require_grant_for<R>(
    state: &HttpState<R>,
    context: &SiteContext,
    grant: Grant,
    resource_type: impl Into<String>,
    resource_id: impl Into<String>,
) -> Result<(), HttpError>
where
    R: SiteResolver,
{
    state
        .authorizer
        .authorize_context(context, grant, resource_type, resource_id, context.site_id)
        .map_err(HttpError)
}

fn status_code(code: ErrorCode) -> StatusCode {
    match code {
        ErrorCode::Validation => StatusCode::BAD_REQUEST,
        ErrorCode::Unauthenticated => StatusCode::UNAUTHORIZED,
        ErrorCode::Forbidden => StatusCode::FORBIDDEN,
        ErrorCode::NotFound => StatusCode::NOT_FOUND,
        ErrorCode::Conflict => StatusCode::CONFLICT,
        ErrorCode::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn error_code(code: ErrorCode) -> String {
    match code {
        ErrorCode::Validation => "validation".to_owned(),
        ErrorCode::Unauthenticated => "unauthenticated".to_owned(),
        ErrorCode::Forbidden => "forbidden".to_owned(),
        ErrorCode::NotFound => "not_found".to_owned(),
        ErrorCode::Conflict => "conflict".to_owned(),
        ErrorCode::RateLimited => "rate_limited".to_owned(),
        ErrorCode::Internal => "internal".to_owned(),
    }
}

fn error_message(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::Validation => "validation failed",
        ErrorCode::Unauthenticated => "authentication required",
        ErrorCode::Forbidden => "operation forbidden",
        ErrorCode::NotFound => "resource not found",
        ErrorCode::Conflict => "operation conflicts with current state",
        ErrorCode::RateLimited => "request rate limited",
        ErrorCode::Internal => "internal error",
    }
}

/// A handler can use this extractor once the admission layer is installed.
pub type SiteExtension = Extension<SiteContext>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_header_requires_a_nonempty_bearer_token() {
        let request = Request::new(axum::body::Body::empty());
        assert_eq!(authorization_token(&request).expect("no header"), None);

        let mut request = Request::new(axum::body::Body::empty());
        request
            .headers_mut()
            .insert(AUTHORIZATION, HeaderValue::from_static("Basic abc"));
        assert!(matches!(
            authorization_token(&request),
            Err(MaviError::Unauthenticated)
        ));

        request
            .headers_mut()
            .insert(AUTHORIZATION, HeaderValue::from_static("Bearer token"));
        assert_eq!(
            authorization_token(&request).expect("bearer"),
            Some("token")
        );
    }

    #[test]
    fn application_api_catalog_combines_domain_contracts() {
        let catalog = api();
        catalog.validate().expect("application API contract");
        assert!(
            catalog
                .endpoints
                .iter()
                .any(|endpoint| endpoint.operation_id == "people.list")
        );
        assert!(
            catalog
                .endpoints
                .iter()
                .any(|endpoint| endpoint.operation_id == "content.list")
        );
        assert!(
            catalog
                .endpoints
                .iter()
                .any(|endpoint| endpoint.operation_id == "content_types.upsert")
        );
    }
}
