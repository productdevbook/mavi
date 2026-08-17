//! HTTP boundary for the clean Mavi implementation.
//!
//! This crate admits a request into a [`SiteContext`] before a handler runs.
//! Handlers receive the context as an extension and cannot silently resolve a
//! different site halfway through an operation.

use std::sync::Arc;

use axum::{
    Extension, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Json, Path, Query, State},
    http::{
        HeaderValue, Request, StatusCode,
        header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
};
use chrono::Utc;
use mavi_audit::{AuditEvent, AuditListFilter, AuditService};
use mavi_authz::CedarAuthorizer;
use mavi_content::{
    Content, ContentListFilter, ContentRevision, ContentRevisionListFilter, ContentService,
    ContentType, ContentTypeListFilter, CreateContent, DeclareContentType, PublicationInput,
    ScheduleContent, UpdateContent,
};
use mavi_contract::Api;
use mavi_core::{
    Action, AuditEventId, Caller, Capability, ContentId, CouponId, CourseId, DesignBuildId,
    DesignChangeId, EnrollmentId, ErrorCode, FileId, FlowId, FlowRunId, FormSubmissionId, Grant,
    JobId, LessonId, MailDeliveryId, MailListId, MailReaderId, MailTemplateId, MaviError, ModuleId,
    OrderId, Page, PersonId, ProductId, RequestId, RoleId, SiteContext, StudentId, TermId,
    ports::FileStore,
};
use mavi_courses::{
    Course, CourseListFilter, CourseSummary, CoursesService, CreateCourse, CreateLesson,
    CreateModule, CreateStudent, EnrollStudent, Enrollment, EnrollmentListFilter, LearningCourse,
    LearningCourseListFilter, LearningLesson, Lesson, LessonListFilter, Module, Progress,
    ReorderLessons, ReorderModules, Student, StudentActivationInput, StudentInvitation,
    StudentListFilter, StudentLoginInput, StudentSessionCreated, UpdateCourse, UpdateLesson,
    UpdateModule, UpdateStudent,
};
use mavi_design::{
    BuildEngine, DESIGN_BUILD_FAILED, DesignBuild, DesignBuildListFilter, DesignChange,
    DesignChangeListFilter, DesignFile, DesignFileInput, DesignFileListFilter, DesignFileQuery,
    DesignService, StartDesignChange,
};
use mavi_flows::{
    CreateFlow, Flow, FlowListFilter, FlowRun, FlowService, RunListFilter, SimulateFlow,
    SimulationStep, TriggerDescription, UpdateFlow,
};
use mavi_forms::{
    CreateForm, Form, FormListFilter, FormService, FormSubmission, PublicForm, SeenCount,
    SubmissionListFilter, SubmissionReceipt, SubmitForm, UpdateForm,
};
use mavi_identity::{
    ApiKeyCreated, CreateApiKey, CreatePerson, CreateRole, IdentityService, LoginInput,
    PeopleListFilter, Person, PersonRecord, ReplaceRoleGrants, Role, RoleListFilter,
    SessionCreated, SetupInput, SetupStatus, UpdatePersonStatus,
};
use mavi_jobs::{Job, JobListFilter, JobsService};
use mavi_mail::{
    AddReader, CreateMailList, CreateMailTemplate, DeliveryListFilter, EnqueueDelivery,
    MailDelivery, MailList, MailListListFilter, MailReader, MailReaderCreated, MailService,
    MailTemplate, MailTemplateListFilter, MailTemplatePreview, ReaderListFilter, RenderedMail,
    RetryDelivery, SendCampaign, SendCount, UnsubscribeReceipt, UpdateMailList, UpdateMailTemplate,
};
use mavi_media::{FileListFilter, FileRecord, MAX_FILE_BYTES, MediaService, UploadFileQuery};
use mavi_runtime::{Runtime, SiteResolver};
use mavi_settings::{
    CreateLanguage, Language, LanguageListFilter, SettingsService, SiteSettings, UpdateLanguage,
    UpdateSiteSettings,
};
use mavi_shop::{
    CheckoutInput, CheckoutReceipt, Coupon, CouponListFilter, CreateCoupon, CreateProduct, Order,
    OrderListFilter, OrderSummary, OrderTransition, Product, ProductListFilter, PublicProduct,
    PublicProductListFilter, ShopService, UpdateProduct,
};
use mavi_taxonomy::{
    ContentTermAssignment, ContentTermAssignmentListFilter, CreateTerm, ReplaceContentTerms,
    TaxonomyService, Term, TermListFilter, UpdateTerm,
};
use mavi_trash::{TrashItem, TrashKind, TrashListFilter, TrashService};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

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
    api.extend(mavi_taxonomy::api());
    api.extend(mavi_media::api());
    api.extend(mavi_audit::api());
    api.extend(mavi_trash::api());
    api.extend(mavi_design::api());
    api.extend(mavi_forms::api());
    api.extend(mavi_mail::api());
    api.extend(mavi_shop::api());
    api.extend(mavi_courses::api());
    api.extend(mavi_jobs::api());
    api.extend(mavi_flows::api());
    api
}

async fn openapi_document() -> Result<Json<Value>, HttpError> {
    api()
        .openapi("Mavi", "0.1.0")
        .map(Json)
        .map_err(|_| HttpError(MaviError::Internal))
}

/// Builds the shared router and admits every request into a site context.
pub fn router<R>(
    runtime: Runtime<R>,
    file_store: Arc<dyn FileStore>,
    builder: Arc<dyn BuildEngine>,
) -> Result<Router, MaviError>
where
    R: SiteResolver,
{
    let state = HttpState {
        runtime: runtime.clone(),
        identity: IdentityService,
        content: ContentService,
        settings: SettingsService,
        taxonomy: TaxonomyService,
        media: MediaService,
        audit: AuditService,
        trash: TrashService,
        design: DesignService,
        forms: FormService,
        mail: MailService,
        shop: ShopService,
        courses: CoursesService,
        jobs: JobsService::new(mavi_flows::job_kinds()),
        flows: FlowService,
        file_store,
        builder,
        authorizer: CedarAuthorizer::new()?,
    };
    let application = runtime
        .router::<HttpState<R>>()
        .merge(api_routes::<R>())
        .layer(middleware::from_fn_with_state(
            runtime.clone(),
            authenticate::<R>,
        ))
        .layer(middleware::from_fn_with_state(runtime, admit::<R>))
        .layer(DefaultBodyLimit::max(MAX_FILE_BYTES + 1));
    Ok(application.with_state(state))
}

fn api_routes<R>() -> Router<HttpState<R>>
where
    R: SiteResolver,
{
    Router::new()
        .route("/openapi.json", get(openapi_document))
        .merge(identity_routes::<R>())
        .merge(settings_routes::<R>())
        .merge(content_routes::<R>())
        .merge(media_routes::<R>())
        .merge(audit_trash_routes::<R>())
        .merge(design_routes::<R>())
        .merge(form_routes::<R>())
        .merge(mail_routes::<R>())
        .merge(course_routes::<R>())
        .merge(shop_routes::<R>())
        .merge(automation_routes::<R>())
}

fn identity_routes<R>() -> Router<HttpState<R>>
where
    R: SiteResolver,
{
    Router::new()
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
}

fn settings_routes<R>() -> Router<HttpState<R>>
where
    R: SiteResolver,
{
    Router::new()
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
}

fn content_routes<R>() -> Router<HttpState<R>>
where
    R: SiteResolver,
{
    Router::new()
        .route("/api/v1/content-types", get(list_content_types::<R>))
        .route(
            "/api/v1/content-types/{kind}",
            put(upsert_content_type::<R>).delete(delete_content_type::<R>),
        )
        .route("/api/v1/terms", get(list_terms::<R>).post(create_term::<R>))
        .route(
            "/api/v1/terms/{id}",
            get(read_term::<R>)
                .patch(update_term::<R>)
                .delete(delete_term::<R>),
        )
        .route("/api/v1/terms/{id}/content", get(list_term_content::<R>))
        .route(
            "/api/v1/content/{id}/terms",
            get(list_content_terms::<R>).put(replace_content_terms::<R>),
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
}

fn media_routes<R>() -> Router<HttpState<R>>
where
    R: SiteResolver,
{
    Router::new()
        .route("/api/v1/files", get(list_files::<R>).post(upload_file::<R>))
        .route(
            "/api/v1/files/{id}",
            get(read_file::<R>).delete(delete_file::<R>),
        )
}

fn audit_trash_routes<R>() -> Router<HttpState<R>>
where
    R: SiteResolver,
{
    Router::new()
        .route("/api/v1/audit", get(list_audit::<R>))
        .route("/api/v1/audit/{id}", get(read_audit::<R>))
        .route("/api/v1/trash", get(list_trash::<R>))
        .route(
            "/api/v1/trash/{kind}/{id}/restore",
            post(restore_trash::<R>),
        )
        .route(
            "/api/v1/trash/{kind}/{id}",
            delete(permanently_delete_trash::<R>),
        )
}

fn design_routes<R>() -> Router<HttpState<R>>
where
    R: SiteResolver,
{
    Router::new()
        .route(
            "/api/v1/design/changes",
            get(list_design_changes::<R>).post(start_design_change::<R>),
        )
        .route("/api/v1/design/changes/{id}", get(read_design_change::<R>))
        .route(
            "/api/v1/design/changes/{id}/files",
            get(list_design_files::<R>),
        )
        .route(
            "/api/v1/design/changes/{id}/file",
            get(read_design_file::<R>)
                .put(write_design_file::<R>)
                .delete(remove_design_file::<R>),
        )
        .route(
            "/api/v1/design/changes/{id}/builds",
            get(list_design_builds::<R>).post(create_design_build::<R>),
        )
        .route(
            "/api/v1/design/changes/{id}/publish",
            post(publish_design_change::<R>),
        )
        .route(
            "/api/v1/design/changes/{id}/rollback",
            post(rollback_design_change::<R>),
        )
        .route(
            "/preview/v1/design/{build_id}/{*path}",
            get(preview_design_asset::<R>),
        )
        .route("/public/v1/site/{*path}", get(public_design_asset::<R>))
}

fn form_routes<R>() -> Router<HttpState<R>>
where
    R: SiteResolver,
{
    Router::new()
        .route("/api/v1/forms", get(list_forms::<R>).post(create_form::<R>))
        .route(
            "/api/v1/forms/{id}",
            get(read_form::<R>)
                .patch(update_form::<R>)
                .delete(delete_form::<R>),
        )
        .route(
            "/api/v1/forms/{id}/submissions",
            get(list_form_submissions::<R>),
        )
        .route(
            "/api/v1/forms/{id}/submissions/mark-read",
            post(mark_form_submissions_read::<R>),
        )
        .route(
            "/api/v1/form-submissions/{id}",
            delete(delete_form_submission::<R>),
        )
        .route("/public/v1/forms/{slug}", get(public_form::<R>))
        .route(
            "/public/v1/forms/{slug}/submissions",
            post(submit_form::<R>),
        )
}

fn mail_routes<R>() -> Router<HttpState<R>>
where
    R: SiteResolver,
{
    Router::new()
        .route(
            "/api/v1/mail/templates",
            get(list_mail_templates::<R>).post(create_mail_template::<R>),
        )
        .route(
            "/api/v1/mail/templates/{id}",
            get(read_mail_template::<R>)
                .patch(update_mail_template::<R>)
                .delete(delete_mail_template::<R>),
        )
        .route(
            "/api/v1/mail/templates/{id}/preview",
            post(preview_mail_template::<R>),
        )
        .route(
            "/api/v1/mail/lists",
            get(list_mail_lists::<R>).post(create_mail_list::<R>),
        )
        .route(
            "/api/v1/mail/lists/{id}",
            get(read_mail_list::<R>)
                .patch(update_mail_list::<R>)
                .delete(delete_mail_list::<R>),
        )
        .route(
            "/api/v1/mail/lists/{id}/readers",
            get(list_mail_readers::<R>).post(add_mail_reader::<R>),
        )
        .route(
            "/api/v1/mail/lists/{id}/deliveries",
            post(send_mail_campaign::<R>),
        )
        .route("/api/v1/mail/readers/{id}", delete(delete_mail_reader::<R>))
        .route(
            "/api/v1/mail/deliveries",
            get(list_mail_deliveries::<R>).post(enqueue_mail_delivery::<R>),
        )
        .route("/api/v1/mail/deliveries/{id}", get(read_mail_delivery::<R>))
        .route(
            "/api/v1/mail/deliveries/{id}/retry",
            post(retry_mail_delivery::<R>),
        )
        .route(
            "/public/v1/mail/unsubscribe/{token}",
            post(public_mail_unsubscribe::<R>),
        )
}

fn course_routes<R>() -> Router<HttpState<R>>
where
    R: SiteResolver,
{
    Router::new()
        .route(
            "/api/v1/courses",
            get(list_courses::<R>).post(create_course::<R>),
        )
        .route(
            "/api/v1/courses/{id}",
            get(read_course::<R>).patch(update_course::<R>),
        )
        .route(
            "/api/v1/courses/{id}/modules/order",
            put(reorder_course_modules::<R>),
        )
        .route(
            "/api/v1/courses/{id}/modules",
            post(create_course_module::<R>),
        )
        .route(
            "/api/v1/courses/modules/{id}",
            get(read_course_module::<R>)
                .patch(update_course_module::<R>)
                .delete(delete_course_module::<R>),
        )
        .route(
            "/api/v1/courses/modules/{id}/lessons",
            get(list_course_lessons::<R>).post(create_course_lesson::<R>),
        )
        .route(
            "/api/v1/courses/modules/{id}/lessons/order",
            put(reorder_course_lessons::<R>),
        )
        .route(
            "/api/v1/courses/lessons/{id}",
            patch(update_course_lesson::<R>).delete(delete_course_lesson::<R>),
        )
        .route(
            "/api/v1/courses/students",
            get(list_course_students::<R>).post(create_course_student::<R>),
        )
        .route(
            "/api/v1/courses/students/{id}",
            axum::routing::patch(update_course_student::<R>),
        )
        .route(
            "/api/v1/courses/students/{id}/invite",
            post(reissue_course_student_invite::<R>),
        )
        .route(
            "/api/v1/courses/{course_id}/enrollments",
            get(list_course_enrollments::<R>).post(enroll_course_student::<R>),
        )
        .route(
            "/api/v1/courses/enrollments/{id}",
            delete(unenroll_course_student::<R>),
        )
        .route(
            "/public/v1/courses/students/activate",
            post(activate_course_student::<R>),
        )
        .route(
            "/public/v1/courses/students/sessions",
            post(login_course_student::<R>),
        )
        .route(
            "/student/v1/auth/session",
            delete(logout_course_student::<R>),
        )
        .route(
            "/student/v1/learning/courses",
            get(list_learning_courses::<R>),
        )
        .route(
            "/student/v1/learning/lessons/{id}",
            get(read_learning_lesson::<R>),
        )
        .route(
            "/student/v1/learning/lessons/{id}/media",
            get(read_learning_lesson_media::<R>),
        )
        .route(
            "/student/v1/learning/lessons/{id}/done",
            put(complete_learning_lesson::<R>),
        )
}

fn shop_routes<R>() -> Router<HttpState<R>>
where
    R: SiteResolver,
{
    Router::new()
        .route(
            "/api/v1/shop/products",
            get(list_shop_products::<R>).post(create_shop_product::<R>),
        )
        .route(
            "/api/v1/shop/products/{id}",
            get(read_shop_product::<R>)
                .patch(update_shop_product::<R>)
                .delete(delete_shop_product::<R>),
        )
        .route(
            "/public/v1/shop/products",
            get(list_public_shop_products::<R>),
        )
        .route(
            "/api/v1/shop/coupons",
            get(list_shop_coupons::<R>).post(create_shop_coupon::<R>),
        )
        .route("/api/v1/shop/coupons/{id}", delete(delete_shop_coupon::<R>))
        .route("/api/v1/shop/orders", get(list_shop_orders::<R>))
        .route("/api/v1/shop/orders/{id}", get(read_shop_order::<R>))
        .route(
            "/api/v1/shop/orders/{id}/transition",
            post(transition_shop_order::<R>),
        )
        .route("/public/v1/shop/orders", post(checkout_shop_order::<R>))
}

fn automation_routes<R>() -> Router<HttpState<R>>
where
    R: SiteResolver,
{
    Router::new()
        .route("/api/v1/jobs", get(list_jobs::<R>))
        .route("/api/v1/jobs/{id}", get(read_job::<R>))
        .route("/api/v1/jobs/{id}/retry", post(retry_job::<R>))
        .route(
            "/api/v1/automation/triggers",
            get(list_automation_triggers::<R>),
        )
        .route(
            "/api/v1/automation/flows",
            get(list_flows::<R>).post(create_flow::<R>),
        )
        .route(
            "/api/v1/automation/flows/{id}",
            get(read_flow::<R>)
                .patch(update_flow::<R>)
                .delete(delete_flow::<R>),
        )
        .route(
            "/api/v1/automation/flows/{id}/simulate",
            post(simulate_flow::<R>),
        )
        .route(
            "/api/v1/automation/flows/{id}/runs",
            get(list_flow_runs::<R>),
        )
        .route("/api/v1/automation/runs/{id}", get(read_flow_run::<R>))
}

struct HttpState<R> {
    runtime: Runtime<R>,
    identity: IdentityService,
    content: ContentService,
    settings: SettingsService,
    taxonomy: TaxonomyService,
    media: MediaService,
    audit: AuditService,
    trash: TrashService,
    design: DesignService,
    forms: FormService,
    mail: MailService,
    shop: ShopService,
    courses: CoursesService,
    jobs: JobsService,
    flows: FlowService,
    file_store: Arc<dyn FileStore>,
    builder: Arc<dyn BuildEngine>,
    authorizer: CedarAuthorizer,
}

impl<R> Clone for HttpState<R> {
    fn clone(&self) -> Self {
        Self {
            runtime: self.runtime.clone(),
            identity: self.identity,
            content: self.content,
            settings: self.settings,
            taxonomy: self.taxonomy,
            media: self.media,
            audit: self.audit,
            trash: self.trash,
            design: self.design,
            forms: self.forms,
            mail: self.mail,
            shop: self.shop,
            courses: self.courses,
            jobs: self.jobs.clone(),
            flows: self.flows,
            file_store: Arc::clone(&self.file_store),
            builder: Arc::clone(&self.builder),
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
        Err(MaviError::Unauthenticated) => match CoursesService
            .authenticate_student(&mut transaction, &public_context, token, Utc::now())
            .await
        {
            Ok(caller) => caller,
            Err(error) => return HttpError(error).into_response(),
        },
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

async fn list_terms<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<TermListFilter>,
) -> Result<Json<Page<Term>>, HttpError>
where
    R: SiteResolver,
{
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

async fn create_term<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateTerm>,
) -> Result<(StatusCode, Json<Term>), HttpError>
where
    R: SiteResolver,
{
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

async fn read_term<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<TermId>,
) -> Result<Json<Term>, HttpError>
where
    R: SiteResolver,
{
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

async fn update_term<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<TermId>,
    Json(input): Json<UpdateTerm>,
) -> Result<Json<Term>, HttpError>
where
    R: SiteResolver,
{
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

async fn delete_term<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<TermId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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

async fn list_content_terms<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
) -> Result<Json<Vec<Term>>, HttpError>
where
    R: SiteResolver,
{
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

async fn replace_content_terms<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ContentId>,
    Json(input): Json<ReplaceContentTerms>,
) -> Result<Json<Vec<Term>>, HttpError>
where
    R: SiteResolver,
{
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

async fn list_term_content<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<TermId>,
    Query(filter): Query<ContentTermAssignmentListFilter>,
) -> Result<Json<Page<ContentTermAssignment>>, HttpError>
where
    R: SiteResolver,
{
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

async fn list_files<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<FileListFilter>,
) -> Result<Json<Page<FileRecord>>, HttpError>
where
    R: SiteResolver,
{
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

async fn upload_file<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(query): Query<UploadFileQuery>,
    body: Bytes,
) -> Result<(StatusCode, Json<FileRecord>), HttpError>
where
    R: SiteResolver,
{
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
            body.to_vec(),
        )
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(file)))
}

async fn read_file<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FileId>,
) -> Result<Json<FileRecord>, HttpError>
where
    R: SiteResolver,
{
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

async fn delete_file<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FileId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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

async fn list_audit<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<AuditListFilter>,
) -> Result<Json<Page<AuditEvent>>, HttpError>
where
    R: SiteResolver,
{
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

async fn read_audit<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<AuditEventId>,
) -> Result<Json<AuditEvent>, HttpError>
where
    R: SiteResolver,
{
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

async fn list_trash<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<TrashListFilter>,
) -> Result<Json<Page<TrashItem>>, HttpError>
where
    R: SiteResolver,
{
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

async fn restore_trash<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path((kind, id)): Path<(String, Uuid)>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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

async fn permanently_delete_trash<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path((kind, id)): Path<(String, Uuid)>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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
    transaction.commit().await.map_err(HttpError)?;

    if let (Some(file_id), Some(storage_key)) = (deletion.file_id, deletion.file_storage_key) {
        state
            .file_store
            .remove(&context, &storage_key)
            .await
            .map_err(HttpError)?;
        let mut cleanup_transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
        state
            .media
            .complete_cleanup(
                &mut cleanup_transaction,
                &context,
                FileId::from_uuid(file_id),
            )
            .await
            .map_err(HttpError)?;
        cleanup_transaction.commit().await.map_err(HttpError)?;
    }

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

async fn list_design_changes<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<DesignChangeListFilter>,
) -> Result<Json<Page<DesignChange>>, HttpError>
where
    R: SiteResolver,
{
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

async fn start_design_change<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<StartDesignChange>,
) -> Result<(StatusCode, Json<DesignChange>), HttpError>
where
    R: SiteResolver,
{
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

async fn read_design_change<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<DesignChangeId>,
) -> Result<Json<DesignChange>, HttpError>
where
    R: SiteResolver,
{
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

async fn list_design_files<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
    Query(filter): Query<DesignFileListFilter>,
) -> Result<Json<Page<mavi_design::DesignFileSummary>>, HttpError>
where
    R: SiteResolver,
{
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

async fn read_design_file<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
    Query(query): Query<DesignFileQuery>,
) -> Result<Json<DesignFile>, HttpError>
where
    R: SiteResolver,
{
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

async fn write_design_file<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
    Json(input): Json<DesignFileInput>,
) -> Result<Json<DesignFile>, HttpError>
where
    R: SiteResolver,
{
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

async fn remove_design_file<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
    Query(query): Query<DesignFileQuery>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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

async fn list_design_builds<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
    Query(filter): Query<DesignBuildListFilter>,
) -> Result<Json<Page<DesignBuild>>, HttpError>
where
    R: SiteResolver,
{
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

async fn create_design_build<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
) -> Result<(StatusCode, Json<DesignBuild>), HttpError>
where
    R: SiteResolver,
{
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
    transaction.commit().await.map_err(HttpError)?;

    let build_id = request.build.id;
    let artifacts = match state
        .builder
        .build(&context, build_id, &request.source)
        .await
    {
        Ok(artifacts) => match state
            .design
            .persist_artifacts(&context, state.file_store.as_ref(), build_id, artifacts)
            .await
        {
            Ok(stored) => stored,
            Err(error) => {
                return finish_failed_design_build(&state, &context, build_id, &error).await;
            }
        },
        Err(error) => {
            return finish_failed_design_build(&state, &context, build_id, &error).await;
        }
    };

    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let build = match state
        .design
        .finish_build_success(&mut transaction, &context, build_id, &artifacts)
        .await
    {
        Ok(build) => build,
        Err(error) => {
            for artifact in &artifacts {
                let _ = state
                    .file_store
                    .remove(&context, &artifact.storage_key)
                    .await;
            }
            return Err(HttpError(error));
        }
    };
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(build)))
}

async fn finish_failed_design_build<R>(
    state: &HttpState<R>,
    context: &SiteContext,
    build_id: DesignBuildId,
    error: &MaviError,
) -> Result<(StatusCode, Json<DesignBuild>), HttpError>
where
    R: SiteResolver,
{
    let error_code = design_build_error_code(error);
    let mut transaction = state.runtime.begin(context).await.map_err(HttpError)?;
    let build = state
        .design
        .finish_build_failed(&mut transaction, context, build_id, &error_code)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(build)))
}

async fn publish_design_change<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
) -> Result<Json<DesignChange>, HttpError>
where
    R: SiteResolver,
{
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

async fn rollback_design_change<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(change_id): Path<DesignChangeId>,
) -> Result<Json<DesignChange>, HttpError>
where
    R: SiteResolver,
{
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

async fn preview_design_asset<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path((build_id, path)): Path<(DesignBuildId, String)>,
) -> Result<Response, HttpError>
where
    R: SiteResolver,
{
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

async fn public_design_asset<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(path): Path<String>,
) -> Result<Response, HttpError>
where
    R: SiteResolver,
{
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

fn design_build_error_code(error: &MaviError) -> String {
    match error {
        MaviError::Validation { code, .. } | MaviError::Conflict { code } => code.clone(),
        MaviError::Unauthenticated
        | MaviError::Forbidden
        | MaviError::NotFound { .. }
        | MaviError::RateLimited
        | MaviError::Internal => DESIGN_BUILD_FAILED.to_owned(),
    }
}

async fn list_forms<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<FormListFilter>,
) -> Result<Json<Page<Form>>, HttpError>
where
    R: SiteResolver,
{
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

async fn create_form<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateForm>,
) -> Result<(StatusCode, Json<Form>), HttpError>
where
    R: SiteResolver,
{
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

async fn read_form<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<mavi_core::FormId>,
) -> Result<Json<Form>, HttpError>
where
    R: SiteResolver,
{
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

async fn update_form<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<mavi_core::FormId>,
    Json(input): Json<UpdateForm>,
) -> Result<Json<Form>, HttpError>
where
    R: SiteResolver,
{
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

async fn delete_form<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<mavi_core::FormId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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

async fn list_form_submissions<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(form_id): Path<mavi_core::FormId>,
    Query(filter): Query<SubmissionListFilter>,
) -> Result<Json<Page<FormSubmission>>, HttpError>
where
    R: SiteResolver,
{
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

async fn mark_form_submissions_read<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(form_id): Path<mavi_core::FormId>,
) -> Result<Json<SeenCount>, HttpError>
where
    R: SiteResolver,
{
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

async fn delete_form_submission<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FormSubmissionId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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

async fn public_form<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(slug): Path<String>,
) -> Result<Json<PublicForm>, HttpError>
where
    R: SiteResolver,
{
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let form = state
        .forms
        .public_get(&mut transaction, &context, &slug)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(form))
}

async fn submit_form<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(slug): Path<String>,
    Json(input): Json<SubmitForm>,
) -> Result<(StatusCode, Json<SubmissionReceipt>), HttpError>
where
    R: SiteResolver,
{
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let receipt = state
        .forms
        .submit(&mut transaction, &context, &slug, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(receipt)))
}

async fn list_mail_templates<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<MailTemplateListFilter>,
) -> Result<Json<Page<MailTemplate>>, HttpError>
where
    R: SiteResolver,
{
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

async fn create_mail_template<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateMailTemplate>,
) -> Result<(StatusCode, Json<MailTemplate>), HttpError>
where
    R: SiteResolver,
{
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

async fn read_mail_template<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailTemplateId>,
) -> Result<Json<MailTemplate>, HttpError>
where
    R: SiteResolver,
{
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

async fn update_mail_template<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailTemplateId>,
    Json(input): Json<UpdateMailTemplate>,
) -> Result<Json<MailTemplate>, HttpError>
where
    R: SiteResolver,
{
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

async fn delete_mail_template<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailTemplateId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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

async fn preview_mail_template<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailTemplateId>,
    Json(input): Json<MailTemplatePreview>,
) -> Result<Json<RenderedMail>, HttpError>
where
    R: SiteResolver,
{
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

async fn list_mail_lists<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<MailListListFilter>,
) -> Result<Json<Page<MailList>>, HttpError>
where
    R: SiteResolver,
{
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

async fn create_mail_list<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateMailList>,
) -> Result<(StatusCode, Json<MailList>), HttpError>
where
    R: SiteResolver,
{
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

async fn read_mail_list<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailListId>,
) -> Result<Json<MailList>, HttpError>
where
    R: SiteResolver,
{
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

async fn update_mail_list<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailListId>,
    Json(input): Json<UpdateMailList>,
) -> Result<Json<MailList>, HttpError>
where
    R: SiteResolver,
{
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

async fn delete_mail_list<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailListId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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

async fn list_mail_readers<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(list_id): Path<MailListId>,
    Query(filter): Query<ReaderListFilter>,
) -> Result<Json<Page<MailReader>>, HttpError>
where
    R: SiteResolver,
{
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

async fn add_mail_reader<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(list_id): Path<MailListId>,
    Json(input): Json<AddReader>,
) -> Result<(StatusCode, Json<MailReaderCreated>), HttpError>
where
    R: SiteResolver,
{
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

async fn delete_mail_reader<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailReaderId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
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

async fn public_mail_unsubscribe<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(token): Path<String>,
) -> Result<Json<UnsubscribeReceipt>, HttpError>
where
    R: SiteResolver,
{
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let receipt = state
        .mail
        .unsubscribe(&mut transaction, &context, &token)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(receipt))
}

async fn list_mail_deliveries<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<DeliveryListFilter>,
) -> Result<Json<Page<MailDelivery>>, HttpError>
where
    R: SiteResolver,
{
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

async fn enqueue_mail_delivery<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<EnqueueDelivery>,
) -> Result<(StatusCode, Json<MailDelivery>), HttpError>
where
    R: SiteResolver,
{
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

async fn read_mail_delivery<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailDeliveryId>,
) -> Result<Json<MailDelivery>, HttpError>
where
    R: SiteResolver,
{
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

async fn retry_mail_delivery<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<MailDeliveryId>,
    Json(_input): Json<RetryDelivery>,
) -> Result<(StatusCode, Json<MailDelivery>), HttpError>
where
    R: SiteResolver,
{
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

async fn send_mail_campaign<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(list_id): Path<MailListId>,
    Json(input): Json<SendCampaign>,
) -> Result<(StatusCode, Json<SendCount>), HttpError>
where
    R: SiteResolver,
{
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
        .send_campaign(&mut transaction, &context, list_id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::ACCEPTED, Json(count)))
}

async fn list_shop_products<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<ProductListFilter>,
) -> Result<Json<Page<Product>>, HttpError>
where
    R: SiteResolver,
{
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::View),
        "ShopProduct",
        "shop_products",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let products = state
        .shop
        .list_products(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(products))
}

async fn create_shop_product<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateProduct>,
) -> Result<(StatusCode, Json<Product>), HttpError>
where
    R: SiteResolver,
{
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::Write),
        "ShopProduct",
        "shop_products",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let product = state
        .shop
        .create_product(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(product)))
}

async fn read_shop_product<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ProductId>,
) -> Result<Json<Product>, HttpError>
where
    R: SiteResolver,
{
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::View),
        "ShopProduct",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let product = state
        .shop
        .get_product(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(product))
}

async fn update_shop_product<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ProductId>,
    Json(input): Json<UpdateProduct>,
) -> Result<Json<Product>, HttpError>
where
    R: SiteResolver,
{
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::Write),
        "ShopProduct",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let product = state
        .shop
        .update_product(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(product))
}

async fn delete_shop_product<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ProductId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::Delete),
        "ShopProduct",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .shop
        .delete_product(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_public_shop_products<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<PublicProductListFilter>,
) -> Result<Json<Page<PublicProduct>>, HttpError>
where
    R: SiteResolver,
{
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let products = state
        .shop
        .list_public_products(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(products))
}

async fn list_shop_coupons<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<CouponListFilter>,
) -> Result<Json<Page<Coupon>>, HttpError>
where
    R: SiteResolver,
{
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::View),
        "ShopCoupon",
        "shop_coupons",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let coupons = state
        .shop
        .list_coupons(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(coupons))
}

async fn create_shop_coupon<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateCoupon>,
) -> Result<(StatusCode, Json<Coupon>), HttpError>
where
    R: SiteResolver,
{
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::Write),
        "ShopCoupon",
        "shop_coupons",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let coupon = state
        .shop
        .create_coupon(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(coupon)))
}

async fn delete_shop_coupon<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<CouponId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::Delete),
        "ShopCoupon",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .shop
        .delete_coupon(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_shop_orders<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<OrderListFilter>,
) -> Result<Json<Page<OrderSummary>>, HttpError>
where
    R: SiteResolver,
{
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::View),
        "ShopOrder",
        "shop_orders",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let orders = state
        .shop
        .list_orders(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(orders))
}

async fn read_shop_order<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<OrderId>,
) -> Result<Json<Order>, HttpError>
where
    R: SiteResolver,
{
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::View),
        "ShopOrder",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let order = state
        .shop
        .get_order(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(order))
}

async fn transition_shop_order<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<OrderId>,
    Json(input): Json<OrderTransition>,
) -> Result<Json<Order>, HttpError>
where
    R: SiteResolver,
{
    require_grant_for(
        &state,
        &context,
        Grant::new(Capability::Shop, Action::Write),
        "ShopOrder",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let order = state
        .shop
        .transition_order(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(order))
}

async fn checkout_shop_order<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CheckoutInput>,
) -> Result<(StatusCode, Json<CheckoutReceipt>), HttpError>
where
    R: SiteResolver,
{
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let receipt = state
        .shop
        .checkout(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(receipt)))
}

async fn list_courses<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<CourseListFilter>,
) -> Result<Json<Page<CourseSummary>>, HttpError>
where
    R: SiteResolver,
{
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

async fn create_course<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateCourse>,
) -> Result<(StatusCode, Json<Course>), HttpError>
where
    R: SiteResolver,
{
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

async fn read_course<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<CourseId>,
) -> Result<Json<Course>, HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(&state, &context, Action::View, "Course", id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let course = state
        .courses
        .get_course(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(course))
}

async fn update_course<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<CourseId>,
    Json(input): Json<UpdateCourse>,
) -> Result<Json<Course>, HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(&state, &context, Action::Write, "Course", id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let course = state
        .courses
        .update_course(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(course))
}

async fn reorder_course_modules<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<CourseId>,
    Json(input): Json<ReorderModules>,
) -> Result<Json<Course>, HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(&state, &context, Action::Write, "Course", id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let course = state
        .courses
        .reorder_modules(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(course))
}

async fn create_course_module<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<CourseId>,
    Json(input): Json<CreateModule>,
) -> Result<(StatusCode, Json<Module>), HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(&state, &context, Action::Write, "Course", id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let module = state
        .courses
        .create_module(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(module)))
}

async fn read_course_module<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ModuleId>,
) -> Result<Json<Module>, HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(
        &state,
        &context,
        Action::View,
        "CourseModule",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let module = state
        .courses
        .get_module(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(module))
}

async fn update_course_module<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ModuleId>,
    Json(input): Json<UpdateModule>,
) -> Result<Json<Module>, HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(
        &state,
        &context,
        Action::Write,
        "CourseModule",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let module = state
        .courses
        .update_module(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(module))
}

async fn delete_course_module<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ModuleId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(
        &state,
        &context,
        Action::Delete,
        "CourseModule",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .courses
        .delete_module(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_course_lessons<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ModuleId>,
    Query(filter): Query<LessonListFilter>,
) -> Result<Json<Page<Lesson>>, HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(
        &state,
        &context,
        Action::View,
        "CourseModule",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let lessons = state
        .courses
        .list_lessons(&mut transaction, &context, id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(lessons))
}

async fn reorder_course_lessons<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ModuleId>,
    Json(input): Json<ReorderLessons>,
) -> Result<Json<Module>, HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(
        &state,
        &context,
        Action::Write,
        "CourseModule",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let module = state
        .courses
        .reorder_lessons(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(module))
}

async fn create_course_lesson<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<ModuleId>,
    Json(input): Json<CreateLesson>,
) -> Result<(StatusCode, Json<Lesson>), HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(
        &state,
        &context,
        Action::Write,
        "CourseModule",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let lesson = state
        .courses
        .create_lesson(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(lesson)))
}

async fn update_course_lesson<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<LessonId>,
    Json(input): Json<UpdateLesson>,
) -> Result<Json<Lesson>, HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(
        &state,
        &context,
        Action::Write,
        "CourseLesson",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let lesson = state
        .courses
        .update_lesson(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(lesson))
}

async fn delete_course_lesson<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<LessonId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(
        &state,
        &context,
        Action::Delete,
        "CourseLesson",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .courses
        .delete_lesson(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_course_students<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<StudentListFilter>,
) -> Result<Json<Page<Student>>, HttpError>
where
    R: SiteResolver,
{
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

async fn create_course_student<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateStudent>,
) -> Result<(StatusCode, Json<StudentInvitation>), HttpError>
where
    R: SiteResolver,
{
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

async fn reissue_course_student_invite<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<StudentId>,
) -> Result<Json<StudentInvitation>, HttpError>
where
    R: SiteResolver,
{
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

async fn update_course_student<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<StudentId>,
    Json(input): Json<UpdateStudent>,
) -> Result<Json<Student>, HttpError>
where
    R: SiteResolver,
{
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

async fn list_course_enrollments<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(course_id): Path<CourseId>,
    Query(filter): Query<EnrollmentListFilter>,
) -> Result<Json<Page<Enrollment>>, HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(
        &state,
        &context,
        Action::View,
        "Course",
        course_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let enrollments = state
        .courses
        .list_enrollments(&mut transaction, &context, course_id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(enrollments))
}

async fn enroll_course_student<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(course_id): Path<CourseId>,
    Json(input): Json<EnrollStudent>,
) -> Result<(StatusCode, Json<Enrollment>), HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(
        &state,
        &context,
        Action::Write,
        "Course",
        course_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let enrollment = state
        .courses
        .enroll(&mut transaction, &context, course_id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(enrollment)))
}

async fn unenroll_course_student<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<EnrollmentId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
    require_courses_grant(
        &state,
        &context,
        Action::Delete,
        "CourseEnrollment",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .courses
        .unenroll(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn activate_course_student<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<StudentActivationInput>,
) -> Result<(StatusCode, Json<StudentSessionCreated>), HttpError>
where
    R: SiteResolver,
{
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let session = state
        .courses
        .activate_student(&mut transaction, &context, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(session)))
}

async fn login_course_student<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<StudentLoginInput>,
) -> Result<(StatusCode, Json<StudentSessionCreated>), HttpError>
where
    R: SiteResolver,
{
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let session = state
        .courses
        .login_student(&mut transaction, &context, &input, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(session)))
}

async fn logout_course_student<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .courses
        .logout_student(&mut transaction, &context, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_learning_courses<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<LearningCourseListFilter>,
) -> Result<Json<Page<LearningCourse>>, HttpError>
where
    R: SiteResolver,
{
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let courses = state
        .courses
        .list_learning_courses(&mut transaction, &context, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(courses))
}

async fn read_learning_lesson<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<LessonId>,
) -> Result<Json<LearningLesson>, HttpError>
where
    R: SiteResolver,
{
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let lesson = state
        .courses
        .get_learning_lesson(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(lesson))
}

async fn read_learning_lesson_media<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<LessonId>,
) -> Result<Response, HttpError>
where
    R: SiteResolver,
{
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

async fn complete_learning_lesson<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<LessonId>,
) -> Result<Json<Progress>, HttpError>
where
    R: SiteResolver,
{
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let progress = state
        .courses
        .complete_lesson(&mut transaction, &context, id, Utc::now())
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(progress))
}

async fn list_jobs<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<JobListFilter>,
) -> Result<Json<Page<Job>>, HttpError>
where
    R: SiteResolver,
{
    require_automation_grant(&state, &context, Action::View, "Job", "job_collection")?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .jobs
        .list(&mut transaction, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn read_job<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<JobId>,
) -> Result<Json<Job>, HttpError>
where
    R: SiteResolver,
{
    require_automation_grant(&state, &context, Action::View, "Job", id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let job = state
        .jobs
        .get(&mut transaction, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(job))
}

async fn retry_job<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<JobId>,
) -> Result<Json<Job>, HttpError>
where
    R: SiteResolver,
{
    require_automation_grant(&state, &context, Action::Write, "Job", id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let job = state
        .jobs
        .retry(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(job))
}

async fn list_automation_triggers<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
) -> Result<Json<Vec<TriggerDescription>>, HttpError>
where
    R: SiteResolver,
{
    require_automation_grant(
        &state,
        &context,
        Action::View,
        "AutomationTrigger",
        "trigger_collection",
    )?;
    Ok(Json(mavi_flows::trigger_descriptions()))
}

async fn list_flows<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<FlowListFilter>,
) -> Result<Json<Page<Flow>>, HttpError>
where
    R: SiteResolver,
{
    require_automation_grant(
        &state,
        &context,
        Action::View,
        "AutomationFlow",
        "flow_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .flows
        .list(&mut transaction, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn create_flow<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateFlow>,
) -> Result<(StatusCode, Json<Flow>), HttpError>
where
    R: SiteResolver,
{
    require_automation_grant(
        &state,
        &context,
        Action::Write,
        "AutomationFlow",
        "flow_collection",
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let flow = state
        .flows
        .create(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(flow)))
}

async fn read_flow<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FlowId>,
) -> Result<Json<Flow>, HttpError>
where
    R: SiteResolver,
{
    require_automation_grant(
        &state,
        &context,
        Action::View,
        "AutomationFlow",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let flow = state
        .flows
        .get(&mut transaction, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(flow))
}

async fn update_flow<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FlowId>,
    Json(input): Json<UpdateFlow>,
) -> Result<Json<Flow>, HttpError>
where
    R: SiteResolver,
{
    require_automation_grant(
        &state,
        &context,
        Action::Write,
        "AutomationFlow",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let flow = state
        .flows
        .update(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(flow))
}

async fn delete_flow<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FlowId>,
) -> Result<StatusCode, HttpError>
where
    R: SiteResolver,
{
    require_automation_grant(
        &state,
        &context,
        Action::Write,
        "AutomationFlow",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .flows
        .delete(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn simulate_flow<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FlowId>,
    Json(input): Json<SimulateFlow>,
) -> Result<Json<Simulation>, HttpError>
where
    R: SiteResolver,
{
    require_automation_grant(
        &state,
        &context,
        Action::View,
        "AutomationFlow",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let steps = state
        .flows
        .simulate(&mut transaction, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(Simulation { steps }))
}

#[derive(Clone, Debug, Serialize)]
struct Simulation {
    steps: Vec<SimulationStep>,
}

async fn list_flow_runs<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FlowId>,
    Query(filter): Query<RunListFilter>,
) -> Result<Json<Page<FlowRun>>, HttpError>
where
    R: SiteResolver,
{
    require_automation_grant(
        &state,
        &context,
        Action::View,
        "AutomationFlow",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .flows
        .list_runs(&mut transaction, id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn read_flow_run<R>(
    State(state): State<HttpState<R>>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<FlowRunId>,
) -> Result<Json<FlowRun>, HttpError>
where
    R: SiteResolver,
{
    require_automation_grant(
        &state,
        &context,
        Action::View,
        "AutomationRun",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let run = state
        .flows
        .get_run(&mut transaction, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(run))
}

fn require_automation_grant<R>(
    state: &HttpState<R>,
    context: &SiteContext,
    action: Action,
    resource_type: impl Into<String>,
    resource_id: impl Into<String>,
) -> Result<(), HttpError>
where
    R: SiteResolver,
{
    require_grant_for(
        state,
        context,
        Grant::new(Capability::Automation, action),
        resource_type,
        resource_id,
    )
}

fn require_courses_grant<R>(
    state: &HttpState<R>,
    context: &SiteContext,
    action: Action,
    resource_type: impl Into<String>,
    resource_id: impl Into<String>,
) -> Result<(), HttpError>
where
    R: SiteResolver,
{
    require_grant_for(
        state,
        context,
        Grant::new(Capability::Courses, action),
        resource_type,
        resource_id,
    )
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
