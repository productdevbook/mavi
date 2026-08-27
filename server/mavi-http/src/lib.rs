//! HTTP boundary for the clean Mavi implementation.
//!
//! This crate admits a request into a [`SiteContext`] before a handler runs.
//! Handlers receive the context as an extension and cannot silently resolve a
//! different site halfway through an operation.

use std::{
    collections::BTreeSet,
    fmt::Write as _,
    future::{Future, ready},
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use axum::{
    Extension, Router,
    body::{Body, Bytes, to_bytes},
    extract::{DefaultBodyLimit, FromRequest, FromRequestParts, Path, State},
    http::{
        HeaderMap, HeaderValue, Request, StatusCode,
        header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, HOST},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
};
use base64::Engine;
use chrono::Utc;
use mavi_analytics::{
    AnalyticsEvent, AnalyticsEventBatch, AnalyticsReceipt, AnalyticsService, DailyAggregate,
    DailyListFilter, EventListFilter, PruneAnalytics, PruneReceipt,
};
use mavi_application::{
    AuthorizationService, HatchetBridgeClient, ImportReceipt, JobKind, PluginRecord, PluginService,
    PortableBundle, PortableImportRequest, PortableService, WorkflowExecutor, WorkflowIntent,
    WorkflowRunListFilter, WorkflowRunRecord, WorkflowScheduler, WorkflowService,
};
use mavi_application::{TrashItem, TrashKind, TrashListFilter, TrashService};
use mavi_audit::{
    AuditEntry, AuditEvent, AuditExport, AuditExportFilter, AuditListFilter, AuditService,
};
use mavi_boards::{
    Activity, ActivityPageFilter, AssignCard, Board, BoardList, BoardListFilter, BoardService,
    Card, CardPageFilter, Comment, CommentPageFilter, CreateBoard, CreateCard, CreateComment,
    CreateList, MoveCard, ReorderLists, UpdateBoard, UpdateCard, UpdateComment,
};
use mavi_content::{
    Content, ContentListFilter, ContentRevision, ContentRevisionListFilter, ContentService,
    ContentType, ContentTypeListFilter, CreateContent, DeclareContentType, PublicationInput,
    SCHEDULED_PUBLISH_JOB, ScheduleContent, UpdateContent,
};
use mavi_contract::{
    Api, Endpoint, InputLocation, Method, Permission as ContractPermission, Shape,
};
use mavi_core::{
    Action, AuditEventId, BoardCardId, BoardCommentId, BoardId, BoardListId, Caller, Capability,
    ContentId, CouponId, CourseId, CredentialId, DesignBuildId, DesignChangeId, EnrollmentId,
    ErrorCode, FileId, FlowId, FlowRunId, FormSubmissionId, Grant, Grants, LessonId,
    MailDeliveryId, MailListId, MailReaderId, MailTemplateId, MaviError, ModuleId, OrderId, Page,
    PageRequest, Permission as BusinessPermission, PersonId, PluginId, ProductId, RequestId,
    RoleId, SiteContext, StudentId, TermId,
    ports::{FileStore, MailContentType, MailMessage, Seals},
};
use mavi_courses::{
    Course, CourseInstructorListFilter, CourseListFilter, CourseSummary, CoursesService,
    CreateCourse, CreateLesson, CreateModule, CreateStudent, EnrollStudent, Enrollment,
    EnrollmentListFilter, LearningCourse, LearningCourseDetail, LearningCourseListFilter,
    LearningLesson, Lesson, LessonListFilter, Module, Progress, ReorderLessons, ReorderModules,
    ReplaceCourseInstructor, Student, StudentActivationInput, StudentInvitation, StudentListFilter,
    StudentLoginInput, StudentSessionCreated, UpdateCourse, UpdateLesson, UpdateModule,
    UpdateStudent,
};
use mavi_design::{
    BuildEngine, DESIGN_BUILD_WORKFLOW, DesignBuild, DesignBuildListFilter, DesignChange,
    DesignChangeListFilter, DesignFile, DesignFileInput, DesignFileListFilter, DesignFileQuery,
    DesignService, StartDesignChange,
};
use mavi_feedback::{CreateReport, FeedbackService, Report, ReportListFilter};
use mavi_flows::{
    CreateFlow, Flow, FlowListFilter, FlowRun, FlowService, RunListFilter, SimulateFlow,
    SimulationStep, TriggerDescription, UpdateFlow,
};
use mavi_forms::{
    CreateForm, Form, FormListFilter, FormService, FormSubmission, FormSubmissionExport,
    PublicForm, SeenCount, SubmissionExportFilter, SubmissionListFilter, SubmissionReceipt,
    SubmitForm, UpdateForm, audit_action as forms_audit_action,
};
use mavi_identity::{
    ApiKeyCreated, ApiKeyListFilter, ApiKeyRecord, CreateApiKey, CreatePerson, CreateRole,
    CurrentSession, EmailVerificationRedeemInput, EmailVerificationRequestInput,
    EmailVerificationRequested, IdentityService, LoginInput, PasswordResetRedeemInput,
    PasswordResetRequestInput, PasswordResetRequested, PeopleListFilter, Person, PersonRecord,
    ReplacePersonRoles, ReplaceRoleGrants, Role, RoleListFilter, SessionCreated, SetupInput,
    SetupStatus, UpdatePersonStatus, audit_action,
};
use mavi_mail::{
    AddReader, CreateMailList, CreateMailTemplate, DeliveryListFilter, EnqueueDelivery,
    MailDelivery, MailList, MailListListFilter, MailProviderEventReceipt, MailReader,
    MailReaderCreated, MailService, MailTemplate, MailTemplateListFilter, MailTemplatePreview,
    ReaderListFilter, ReceiveMailProviderEvent, RenderedMail, RetryDelivery, SendCampaign,
    SendCount, UnsubscribeReceipt, UpdateMailList, UpdateMailTemplate,
};
use mavi_media::{
    FileListFilter, FileRecord, FileVariant, FileVariantListFilter, MAX_FILE_BYTES,
    MEDIA_CLEANUP_JOB, MEDIA_VARIANT_JOB, MediaService, UploadFileQuery, VariantPreset,
};
use mavi_observability::RuntimeMetrics;
use mavi_portable as portable_contract;
use mavi_runtime::{RuntimeManifest, SiteRuntime};
use mavi_secrets::{
    CreateCredential, Credential, CredentialListFilter, CredentialService, RotateCredential,
};
use mavi_settings::{
    CreateLanguage, Language, LanguageListFilter, SettingsService, SiteSettings, UpdateLanguage,
    UpdateSiteSettings,
};
use mavi_shop::{
    CheckoutInput, CheckoutReceipt, Coupon, CouponListFilter, CreateCoupon, CreateProduct, Order,
    OrderListFilter, OrderSummary, OrderTransition, Product, ProductListFilter, PublicProduct,
    PublicProductListFilter, ShopService, UpdateProduct,
};
use mavi_storage::SiteTx;
use mavi_taxonomy::{
    ContentTermAssignment, ContentTermAssignmentListFilter, CreateTerm, ReplaceContentTerms,
    TaxonomyService, Term, TermListFilter, UpdateTerm,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

mod edge;
mod routes;

pub use edge::{
    EdgeAction, EdgeRateLimiter, EdgeSecurityConfig, EdgeThrottlePolicy, TrustedProxySet,
};

const REQUEST_ID_HEADER: &str = "x-request-id";
const MCP_PROTOCOL_VERSION: &str = "2026-07-28";
const MCP_PROTOCOL_HEADER: &str = "MCP-Protocol-Version";
const MCP_METHOD_HEADER: &str = "Mcp-Method";
const MCP_NAME_HEADER: &str = "Mcp-Name";
const MCP_SERVER_INFO_META: &str = "io.modelcontextprotocol/serverInfo";
const MAIL_PROVIDER_EVENTS_PATH: &str = "/internal/v1/mail/provider-events";
const MCP_TOOLS_PAGE_SIZE: usize = 64;
const MCP_TOOLS_CACHE_TTL_MS: u64 = 60_000;

#[derive(Clone, Debug, Deserialize)]
struct McpRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

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

/// JSON boundary used by every HTTP handler.
///
/// Axum's normal JSON extractor deliberately follows Serde's default of
/// ignoring unknown object keys. That is a poor API contract: a typo in a
/// client request becomes a successful request with a different meaning.
/// This wrapper keeps the transport behavior in one place and rejects the
/// first unknown field with the same machine-readable error envelope as the
/// rest of the API. Nested `serde_json::Value` fields remain intentionally
/// open when their domain schema says they are open.
#[derive(Debug, Clone, Copy, Default)]
pub struct Json<T>(pub T);

impl<T, S> FromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(
        request: Request<Body>,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        if !is_json_content_type(request.headers()) {
            return Err(input_rejection(MaviError::validation(
                "json_content_type_required",
            )));
        }

        let bytes = Bytes::from_request(request, state)
            .await
            .map_err(|_| input_rejection(MaviError::validation("invalid_json")))?;
        let mut deserializer = serde_json::Deserializer::from_slice(&bytes);
        let mut unknown_field = None;
        let value = serde_ignored::deserialize(&mut deserializer, |path| {
            if unknown_field.is_none() {
                unknown_field = Some(path.to_string());
            }
        })
        .map_err(|_| input_rejection(MaviError::validation("invalid_json")))?;
        deserializer
            .end()
            .map_err(|_| input_rejection(MaviError::validation("invalid_json")))?;

        if let Some(field) = unknown_field {
            return Err(input_rejection(MaviError::validation_field(
                "unknown_field",
                field,
            )));
        }

        Ok(Self(value))
    }
}

impl<T> IntoResponse for Json<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

/// Query boundary paired with [`Json`]. Query structs are subject to the same
/// closed-shape rule, including flattened cursor pagination fields.
#[derive(Debug, Clone, Copy, Default)]
pub struct Query<T>(pub T);

impl<T, S> FromRequestParts<S> for Query<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = Response;

    fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> impl Future<Output = std::result::Result<Self, Self::Rejection>> + Send {
        let query = parts.uri.query().unwrap_or_default();
        let deserializer =
            serde_urlencoded::Deserializer::new(form_urlencoded::parse(query.as_bytes()));
        let mut unknown_field = None;
        let Ok(value) = serde_ignored::deserialize(deserializer, |path| {
            if unknown_field.is_none() {
                unknown_field = Some(path.to_string());
            }
        }) else {
            return ready(Err(input_rejection(MaviError::validation("invalid_query"))));
        };

        if let Some(field) = unknown_field {
            return ready(Err(input_rejection(MaviError::validation_field(
                "unknown_field",
                field,
            ))));
        }

        ready(Ok(Self(value)))
    }
}

impl<T> std::ops::Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::Deref for Query<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn input_rejection(error: MaviError) -> Response {
    let status = status_code(error.code());
    (status, axum::Json(ErrorEnvelope::from(error))).into_response()
}

fn is_json_content_type(headers: &HeaderMap) -> bool {
    let Some(value) = headers.get(CONTENT_TYPE) else {
        return false;
    };
    let Ok(value) = value.to_str() else {
        return false;
    };
    let media_type = value.split(';').next().unwrap_or_default().trim();
    media_type == "application/json"
        || (media_type.starts_with("application/") && media_type.ends_with("+json"))
}

/// Returns the complete site API catalog used by documentation and clients.
///
/// Each domain owns its endpoint declarations; the HTTP composition root is
/// the only place that combines them into the application contract.
#[must_use]
pub fn api() -> Api {
    let mut api = mavi_identity::api();
    extend_plugin_api(&mut api, PluginId::Writing, mavi_content::api());
    api.extend(mavi_settings::api());
    extend_plugin_api(&mut api, PluginId::Writing, mavi_taxonomy::api());
    extend_plugin_api(&mut api, PluginId::Writing, mavi_media::api());
    extend_plugin_api(&mut api, PluginId::Governance, mavi_audit::api());
    extend_plugin_api(
        &mut api,
        PluginId::Governance,
        mavi_application::trash::api(),
    );
    extend_plugin_api(&mut api, PluginId::Writing, mavi_design::api());
    extend_plugin_api(&mut api, PluginId::Forms, mavi_forms::api());
    extend_plugin_api(&mut api, PluginId::Core, mavi_feedback::api());
    extend_plugin_api(&mut api, PluginId::Messaging, mavi_mail::api());
    extend_plugin_api(&mut api, PluginId::Commerce, mavi_shop::api());
    extend_plugin_api(&mut api, PluginId::Learning, mavi_courses::api());
    extend_plugin_api(&mut api, PluginId::Automation, mavi_flows::api());
    extend_plugin_api(&mut api, PluginId::Boards, mavi_boards::api());
    extend_plugin_api(&mut api, PluginId::Analytics, mavi_analytics::api());
    extend_plugin_api(&mut api, PluginId::Governance, portable_contract::api());
    api.extend(mavi_secrets::api());
    api.extend(plugin_api());
    api.extend(workflow_api());
    api.extend(runtime_api());
    api
}

fn extend_plugin_api(api: &mut Api, plugin: PluginId, mut extension: Api) {
    for endpoint in &mut extension.endpoints {
        // Domain APIs normally start with the core default. Preserve an
        // explicit cross-plugin ownership declaration (for example content
        // trash/restore belongs to governance), while assigning the domain's
        // plugin to its ordinary endpoints.
        if endpoint.required_plugin.is_core() {
            endpoint.required_plugin = plugin;
        }
    }
    api.extend(extension);
}

fn plugin_api() -> Api {
    Api::new([
        Endpoint::new(
            Method::Get,
            "/api/v1/plugins",
            "plugins.list",
            "List compiled plugins and activation state",
        )
        .requires(ContractPermission::new(PluginId::Core, "plugins.list"))
        .for_plugin(PluginId::Core)
        .returns(200, "PluginRecordPage"),
        Endpoint::new(
            Method::Post,
            "/api/v1/plugins/{id}/enable",
            "plugins.enable",
            "Enable a compiled plugin",
        )
        .requires(ContractPermission::new(PluginId::Core, "plugins.activate"))
        .for_plugin(PluginId::Core)
        .changes(false)
        .returns(200, "PluginRecord"),
        Endpoint::new(
            Method::Post,
            "/api/v1/plugins/{id}/disable",
            "plugins.disable",
            "Disable a compiled plugin without deleting its data",
        )
        .requires(ContractPermission::new(PluginId::Core, "plugins.deactivate"))
        .for_plugin(PluginId::Core)
        .changes(false)
        .returns(200, "PluginRecord"),
    ])
    .with_shapes([Shape::new(
        "PluginRecord",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["id", "version", "dependencies", "default_enabled", "enabled", "config", "updated_at"],
            "properties": {
                "id": {"type": "string"},
                "version": {"type": "string"},
                "dependencies": {"type": "array", "items": {"type": "string"}},
                "default_enabled": {"type": "boolean"},
                "enabled": {"type": "boolean"},
                "config": {"type": "object"},
                "updated_at": {"type": "string", "format": "date-time"}
            }
        }),
    ),
        Shape::new(
            "PluginRecordPage",
            json!({
                "type": "array",
                "items": {"$ref": "#/components/schemas/PluginRecord"}
            }),
        ),
    ])
}

#[allow(clippy::too_many_lines)]
fn workflow_api() -> Api {
    Api::new([
        Endpoint::new(
            Method::Get,
            "/api/v1/workflows/runs",
            "workflows.runs.list",
            "List durable workflow runs",
        )
        .requires(ContractPermission::new(
            PluginId::Automation,
            "workflow.view",
        ))
        .for_plugin(PluginId::Automation)
        .takes_query("WorkflowRunListFilter")
        .returns(200, "WorkflowRunPage"),
        Endpoint::new(
            Method::Get,
            "/api/v1/workflows/runs/{id}",
            "workflows.runs.read",
            "Read a durable workflow run",
        )
        .requires(ContractPermission::new(
            PluginId::Automation,
            "workflow.view",
        ))
        .for_plugin(PluginId::Automation)
        .returns(200, "WorkflowRun"),
        Endpoint::new(
            Method::Post,
            "/api/v1/workflows/runs/{id}/cancel",
            "workflows.runs.cancel",
            "Cancel a durable workflow run",
        )
        .requires(ContractPermission::new(
            PluginId::Automation,
            "workflow.control",
        ))
        .for_plugin(PluginId::Automation)
        .changes(false)
        .returns(200, "WorkflowRun"),
        Endpoint::new(
            Method::Post,
            "/api/v1/workflows/runs/{id}/replay",
            "workflows.runs.replay",
            "Replay a durable workflow run",
        )
        .requires(ContractPermission::new(
            PluginId::Automation,
            "workflow.control",
        ))
        .for_plugin(PluginId::Automation)
        .changes(false)
        .returns(200, "WorkflowRun"),
        Endpoint::new(
            Method::Post,
            "/api/v1/workflows/runs/{id}/pause",
            "workflows.runs.pause",
            "Pause a durable workflow run",
        )
        .requires(ContractPermission::new(
            PluginId::Automation,
            "workflow.control",
        ))
        .for_plugin(PluginId::Automation)
        .changes(false)
        .returns(200, "WorkflowRun"),
        Endpoint::new(
            Method::Post,
            "/api/v1/workflows/runs/{id}/resume",
            "workflows.runs.resume",
            "Resume a paused durable workflow run",
        )
        .requires(ContractPermission::new(
            PluginId::Automation,
            "workflow.control",
        ))
        .for_plugin(PluginId::Automation)
        .changes(false)
        .returns(200, "WorkflowRun"),
    ])
    .with_shapes([
        Shape::new(
            "WorkflowRun",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "site_id", "plugin", "workflow", "hatchet_run_id", "status", "created_at", "updated_at"],
                "properties": {
                    "id": {"type": "string"},
                    "site_id": {"type": "string", "format": "uuid"},
                    "plugin": {"type": "string"},
                    "workflow": {"type": "string"},
                    "hatchet_run_id": {"type": ["string", "null"]},
                    "status": {"type": "string"},
                    "created_at": {"type": "string", "format": "date-time"},
                    "updated_at": {"type": "string", "format": "date-time"}
                }
            }),
        ),
        Shape::new(
            "WorkflowRunListFilter",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100}
                }
            }),
        ),
        Shape::new(
            "WorkflowRunPage",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/WorkflowRun"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512}
                }
            }),
        ),
    ])
}

fn runtime_api() -> Api {
    Api::new([Endpoint::new(
        Method::Get,
        "/api/v1/runtime/manifest",
        "runtime.manifest.read",
        "Read the runtime compatibility manifest",
    )
    .public()
    .returns(200, "RuntimeManifest")])
    .with_shapes([
        Shape::new(
            "RuntimeManifest",
            serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "required": [
                    "protocol",
                    "release",
                    "api_contract_version",
                    "api_contract_hash",
                    "storage_schema_version",
                    "site_id",
                    "active_plugins",
                    "pagination"
                ],
                "properties": {
                    "protocol": {"type": "string", "const": "mavi.runtime.v1"},
                    "release": {"type": "string"},
                    "api_contract_version": {"type": "string", "const": "v1"},
                    "api_contract_hash": {"type": "string", "pattern": "^sha256:[0-9a-f]{64}$"},
                    "storage_schema_version": {"type": "integer", "minimum": 1},
                    "site_id": {"type": "string", "format": "uuid"},
                    "active_plugins": {"type": "array", "items": {"type": "string"}},
                    "pagination": {"$ref": "#/components/schemas/PaginationContract"}
                }
            }),
        ),
        Shape::new(
            "PaginationContract",
            serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["style", "default_limit", "max_limit"],
                "properties": {
                    "style": {"type": "string", "const": "cursor"},
                    "default_limit": {"type": "integer", "minimum": 1, "maximum": 100},
                    "max_limit": {"type": "integer", "minimum": 1, "maximum": 100}
                }
            }),
        ),
    ])
}

async fn openapi_document(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
) -> Result<Json<Value>, HttpError> {
    active_api(&state, &context)
        .await
        .map_err(HttpError)?
        .openapi("Mavi", mavi_contract::API_VERSION)
        .map(Json)
        .map_err(|_| HttpError(MaviError::Internal))
}

/// Builds the shared router and admits every request into a site context.
pub fn router(
    runtime: SiteRuntime,
    file_store: Arc<dyn FileStore>,
    builder: Arc<dyn BuildEngine>,
    sealer: Arc<dyn Seals>,
) -> Result<Router, MaviError> {
    router_with_config(
        runtime,
        file_store,
        builder,
        sealer,
        EdgeSecurityConfig::default(),
    )
}

/// Builds the shared router with explicit edge trust and throttling policy.
///
/// Self-host callers can use [`router`] for the safe default. Cloud or
/// reverse-proxy deployments should use this constructor when they have an
/// allowlisted proxy network from which forwarded client IP headers may be
/// trusted.
pub fn router_with_config(
    runtime: SiteRuntime,
    file_store: Arc<dyn FileStore>,
    builder: Arc<dyn BuildEngine>,
    sealer: Arc<dyn Seals>,
    edge: EdgeSecurityConfig,
) -> Result<Router, MaviError> {
    router_with_config_and_metrics(
        runtime,
        file_store,
        builder,
        sealer,
        edge,
        RuntimeMetrics::default(),
    )
}

/// Builds the shared router with an explicit edge policy and process metrics
/// registry. Composition roots should pass the same registry to the worker
/// supervisor so `/metrics` exposes HTTP and background-job counters together.
pub fn router_with_config_and_metrics(
    runtime: SiteRuntime,
    file_store: Arc<dyn FileStore>,
    builder: Arc<dyn BuildEngine>,
    sealer: Arc<dyn Seals>,
    edge: EdgeSecurityConfig,
    metrics: RuntimeMetrics,
) -> Result<Router, MaviError> {
    router_with_config_and_metrics_and_mail_webhook(
        runtime, file_store, builder, sealer, edge, metrics, None,
    )
}

/// Builds the shared router and optionally admits a deployment-configured
/// normalized mail provider webhook. The webhook credential is deliberately
/// separate from account/API-key authentication: a provider callback is a
/// site-scoped system event, not a human session.
pub fn router_with_config_and_metrics_and_mail_webhook(
    runtime: SiteRuntime,
    file_store: Arc<dyn FileStore>,
    builder: Arc<dyn BuildEngine>,
    sealer: Arc<dyn Seals>,
    edge: EdgeSecurityConfig,
    metrics: RuntimeMetrics,
    mail_webhook_token: Option<Arc<str>>,
) -> Result<Router, MaviError> {
    router_with_config_and_metrics_and_mail_webhook_and_workflow_executor(
        runtime,
        file_store,
        builder,
        sealer,
        edge,
        metrics,
        mail_webhook_token,
        None,
    )
}

/// Builds the HTTP router with an optional Rust workflow executor.
///
/// The API-only role leaves the private executor route unavailable. The
/// all-in-one role injects the same worker implementation used by the split
/// worker process, so Hatchet never needs to know Mavi business logic.
#[allow(clippy::too_many_arguments)]
pub fn router_with_config_and_metrics_and_mail_webhook_and_workflow_executor(
    runtime: SiteRuntime,
    file_store: Arc<dyn FileStore>,
    builder: Arc<dyn BuildEngine>,
    sealer: Arc<dyn Seals>,
    edge: EdgeSecurityConfig,
    metrics: RuntimeMetrics,
    mail_webhook_token: Option<Arc<str>>,
    workflow_executor: Option<Arc<dyn WorkflowExecutor>>,
) -> Result<Router, MaviError> {
    let mcp_dispatcher = Arc::new(OnceLock::new());
    let plugin_registry = mavi_application::PluginRegistry::built_in();
    let hatchet_bridge = HatchetBridgeClient::from_env()?;
    let state = HttpState {
        runtime: runtime.clone(),
        plugins: PluginService::new(plugin_registry.clone()),
        authorization: AuthorizationService::new_with_plugin_policies_and_observer(
            &plugin_registry,
            Some(Arc::new(metrics.clone())),
        )?,
        workflows: WorkflowService,
        identity: IdentityService,
        content: ContentService,
        settings: SettingsService,
        taxonomy: TaxonomyService,
        media: MediaService,
        audit: AuditService,
        trash: TrashService,
        design: DesignService,
        forms: FormService,
        feedback: FeedbackService,
        mail: MailService,
        shop: ShopService,
        courses: CoursesService,
        jobs: WorkflowScheduler::new(mavi_flows::job_kinds().into_iter().chain([
            SCHEDULED_PUBLISH_JOB,
            MEDIA_CLEANUP_JOB,
            MEDIA_VARIANT_JOB,
            JobKind::new(DESIGN_BUILD_WORKFLOW, 5),
        ])),
        flows: FlowService,
        boards: BoardService,
        analytics: AnalyticsService,
        portable: PortableService::new(),
        credentials: CredentialService,
        file_store,
        builder,
        sealer,
        edge,
        mcp_dispatcher: Arc::clone(&mcp_dispatcher),
        metrics: metrics.clone(),
        mail_webhook_token,
        hatchet_bridge,
        bridge_secret: std::env::var("MAVI_HATCHET_BRIDGE_SECRET")
            .ok()
            .map(Arc::<str>::from),
        workflow_executor,
    };
    let routes = Router::<HttpState>::new().merge(api_routes());
    spawn_plugin_activation_listener(runtime.database(), state.plugins.clone());
    let api_only = routes
        .clone()
        .layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .layer(middleware::from_fn_with_state(state.clone(), edge_throttle))
        .layer(middleware::from_fn_with_state(state.clone(), plugin_gate))
        .layer(middleware::from_fn_with_state(runtime.clone(), admit))
        .layer(DefaultBodyLimit::max(MAX_FILE_BYTES + 1))
        .with_state(state.clone());
    mcp_dispatcher
        .set(api_only)
        .map_err(|_| MaviError::Internal)?;
    let scoped_routes = routes
        .route("/mcp", post(mcp_endpoint))
        .layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .layer(middleware::from_fn_with_state(state.clone(), edge_throttle))
        .layer(middleware::from_fn_with_state(state.clone(), plugin_gate))
        .layer(middleware::from_fn_with_state(runtime.clone(), admit))
        .layer(DefaultBodyLimit::max(MAX_FILE_BYTES + 1));
    let internal_routes = Router::<HttpState>::new()
        .route("/internal/v1/workflows/execute", post(execute_workflow))
        .layer(middleware::from_fn(workflow_executor_auth))
        .layer(Extension(WorkflowExecutorContext {
            runtime: runtime.clone(),
            secret: if state.workflow_executor.is_some() {
                state.bridge_secret.clone()
            } else {
                None
            },
        }))
        .layer(DefaultBodyLimit::max(64 * 1024));
    let operational_routes = Router::<HttpState>::new()
        .route("/healthz", get(liveness))
        .route("/readyz", get(readiness))
        .route("/metrics", get(metrics_endpoint))
        .layer(Extension(runtime));

    Ok(operational_routes
        .merge(scoped_routes)
        .merge(internal_routes)
        .with_state(state)
        .layer(middleware::from_fn_with_state(metrics, request_telemetry)))
}

#[derive(Clone)]
struct WorkflowExecutorContext {
    runtime: SiteRuntime,
    secret: Option<Arc<str>>,
}

#[derive(Clone)]
struct ExecutorState {
    runtime: SiteRuntime,
    plugins: PluginService,
    workflows: WorkflowService,
    executor: Arc<dyn WorkflowExecutor>,
}

/// Creates the private Rust executor listener used by `MAVI_PROCESS_ROLE=worker`.
/// It has no public API routes and accepts only the bridge credential.
pub fn workflow_executor_router(
    runtime: SiteRuntime,
    executor: Arc<dyn WorkflowExecutor>,
    bridge_secret: Arc<str>,
) -> Router {
    let state = ExecutorState {
        runtime: runtime.clone(),
        plugins: PluginService::default(),
        workflows: WorkflowService,
        executor,
    };
    Router::<ExecutorState>::new()
        .route(
            "/internal/v1/workflows/execute",
            post(execute_workflow_private),
        )
        .layer(middleware::from_fn(workflow_executor_auth))
        .layer(Extension(WorkflowExecutorContext {
            runtime,
            secret: Some(bridge_secret),
        }))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .with_state(state)
}

async fn liveness() -> StatusCode {
    StatusCode::OK
}

async fn readiness(Extension(runtime): Extension<SiteRuntime>) -> StatusCode {
    if runtime.ready().await.is_ok() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn metrics_endpoint(State(state): State<HttpState>) -> Response {
    let mut response = Response::new(Body::from(state.metrics.prometheus()));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    response
}

/// Emits one structured completion event for every request at the outer HTTP
/// boundary. The middleware deliberately does not know about a domain or a
/// site resolver; admission remains responsible for enriching the request
/// with its [`SiteContext`]. Keeping this concern here also covers global
/// liveness/readiness probes, which never enter site admission.
async fn request_telemetry(
    State(metrics): State<RuntimeMetrics>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .copied()
        .unwrap_or_else(RequestId::new);
    request.extensions_mut().insert(request_id);

    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let started = Instant::now();
    let mut response = next.run(request).await;
    let request_id_header = HeaderValue::from_str(&request_id.to_string())
        .expect("UUID request IDs are always valid header values");
    response
        .headers_mut()
        .entry(REQUEST_ID_HEADER)
        .or_insert(request_id_header);
    metrics.record_http_response(response.status().as_u16());

    tracing::info!(
        %request_id,
        method = %method,
        path,
        status = response.status().as_u16(),
        duration_ms = started.elapsed().as_millis(),
        "http request completed"
    );

    response
}

fn api_routes() -> Router<HttpState> {
    routes::api_routes()
}

async fn active_plugins(
    state: &HttpState,
    context: &SiteContext,
) -> Result<BTreeSet<PluginId>, MaviError> {
    let mut transaction = state.runtime.begin(context).await?;
    let active = state.plugins.enabled_set(&mut transaction).await?;
    transaction.commit().await?;
    state.authorization.set_active_plugins(active.clone());
    Ok(active)
}

/// Invalidates the process-local plugin snapshot when another API/worker
/// process changes activation. `PostgreSQL` remains the source of truth; the
/// cache only removes a read on the hot path and is always dropped on a
/// notification or listener reconnect.
fn spawn_plugin_activation_listener(database: mavi_storage::Database, plugins: PluginService) {
    tokio::spawn(async move {
        loop {
            match database.listen("mavi_plugin_changed").await {
                Ok(mut listener) => loop {
                    match listener.recv().await {
                        Ok(_) => plugins.invalidate(),
                        Err(error) => {
                            tracing::warn!(error = ?error, "plugin activation listener disconnected");
                            plugins.invalidate();
                            break;
                        }
                    }
                },
                Err(error) => {
                    tracing::warn!(error = ?error, "plugin activation listener unavailable");
                    plugins.invalidate();
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

async fn active_api(state: &HttpState, context: &SiteContext) -> Result<Api, MaviError> {
    let active = active_plugins(state, context).await?;
    Ok(api().for_plugins(&active))
}

/// Runtime route gate shared by HTTP and MCP dispatch. The router remains
/// compiled for every built-in plugin, but disabled plugin prefixes are a
/// backend 404 and never reach authentication or business handlers.
async fn plugin_gate(
    State(state): State<HttpState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let Some(context) = request.extensions().get::<SiteContext>() else {
        return HttpError(MaviError::Internal).into_response();
    };
    let plugin = route_plugin_for_path(&state, request.method(), request.uri().path());
    match active_plugins(&state, context).await {
        Ok(active) if active.contains(&plugin) => next.run(request).await,
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => HttpError(error).into_response(),
    }
}

/// Resolve the plugin from the same endpoint catalog that generates `OpenAPI`,
/// MCP and the client contract. Descriptor prefixes remain a fallback for
/// wildcard/static transport routes that are intentionally not public API
/// operations.
fn route_plugin_for_path(state: &HttpState, method: &axum::http::Method, path: &str) -> PluginId {
    let method = contract_method(method);
    api()
        .endpoints
        .iter()
        .filter(|endpoint| {
            endpoint_path_matches(&endpoint.path, path)
                && method.is_some_and(|method| endpoint.method == method)
        })
        .max_by_key(|endpoint| {
            endpoint
                .path
                .split('/')
                .filter(|segment| !segment.is_empty() && !segment.starts_with('{'))
                .count()
        })
        .map_or_else(
            || state.plugins.registry.plugin_for_path(path),
            |endpoint| endpoint.required_plugin,
        )
}

fn contract_method(method: &axum::http::Method) -> Option<Method> {
    if *method == axum::http::Method::GET || *method == axum::http::Method::HEAD {
        Some(Method::Get)
    } else if *method == axum::http::Method::POST {
        Some(Method::Post)
    } else if *method == axum::http::Method::PUT {
        Some(Method::Put)
    } else if *method == axum::http::Method::PATCH {
        Some(Method::Patch)
    } else if *method == axum::http::Method::DELETE {
        Some(Method::Delete)
    } else {
        None
    }
}

fn endpoint_path_matches(template: &str, path: &str) -> bool {
    let mut template = template.split('/').filter(|segment| !segment.is_empty());
    let mut path = path.split('/').filter(|segment| !segment.is_empty());
    loop {
        match (template.next(), path.next()) {
            (None, None) => return true,
            (Some(segment), Some(value))
                if (segment.starts_with('{') && segment.ends_with('}')) || segment == value => {}
            _ => return false,
        }
    }
}

async fn workflow_executor_auth(mut request: Request<Body>, next: Next) -> Response {
    let Some(executor_context) = request
        .extensions()
        .get::<WorkflowExecutorContext>()
        .cloned()
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(expected) = executor_context.secret.as_deref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(value) = request.headers().get(AUTHORIZATION) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(value) = value.to_str() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !secrets_equal(expected, token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .copied()
        .unwrap_or_else(RequestId::new);
    request.extensions_mut().insert(SiteContext::system(
        executor_context.runtime.site_id(),
        "hatchet-bridge",
        request_id,
    ));
    next.run(request).await
}

async fn execute_workflow(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(intent): Json<WorkflowIntent>,
) -> Result<StatusCode, HttpError> {
    let executor = state
        .workflow_executor
        .clone()
        .ok_or(HttpError(MaviError::Internal))?;
    execute_workflow_intent(
        &state.runtime,
        &state.plugins,
        &state.workflows,
        executor,
        &context,
        intent,
    )
    .await
}

async fn execute_workflow_private(
    State(state): State<ExecutorState>,
    Extension(context): Extension<SiteContext>,
    Json(intent): Json<WorkflowIntent>,
) -> Result<StatusCode, HttpError> {
    execute_workflow_intent(
        &state.runtime,
        &state.plugins,
        &state.workflows,
        Arc::clone(&state.executor),
        &context,
        intent,
    )
    .await
}

async fn execute_workflow_intent(
    runtime: &SiteRuntime,
    plugins: &PluginService,
    workflows: &WorkflowService,
    executor: Arc<dyn WorkflowExecutor>,
    context: &SiteContext,
    intent: WorkflowIntent,
) -> Result<StatusCode, HttpError> {
    intent.validate().map_err(HttpError)?;
    if intent.site_id != context.site_id || intent.site_id != runtime.site_id() {
        return Err(HttpError(MaviError::Forbidden));
    }
    let mut transaction = runtime.begin(context).await.map_err(HttpError)?;
    // The private split worker has a process-local PluginService that does
    // not share the API listener connection. Drop its snapshot before every
    // Hatchet delivery so disabling a plugin takes effect immediately even if
    // the worker missed a PostgreSQL NOTIFY while reconnecting.
    plugins.invalidate();
    let active = plugins
        .enabled_set(&mut transaction)
        .await
        .map_err(HttpError)?;
    if !active.contains(&intent.plugin) {
        // The relay normally cancels disabled intents before publishing them,
        // but a plugin can be disabled while a Hatchet run is already in
        // flight. Stop that delivery cleanly so Hatchet does not spend its
        // retry budget on work the site explicitly turned off.
        match workflows
            .get_run(&mut transaction, &intent.idempotency_key)
            .await
        {
            Ok(run) if !matches!(run.status.as_str(), "completed" | "cancelled") => {
                workflows
                    .cancel(&mut transaction, &intent.idempotency_key)
                    .await
                    .map_err(HttpError)?;
            }
            Ok(_)
            | Err(MaviError::NotFound {
                resource: "workflow_run",
            }) => {}
            Err(error) => return Err(HttpError(error)),
        }
        transaction.commit().await.map_err(HttpError)?;
        return Ok(StatusCode::ACCEPTED);
    }
    match workflows
        .get_run(&mut transaction, &intent.idempotency_key)
        .await
    {
        Ok(run) if matches!(run.status.as_str(), "completed" | "cancelled" | "paused") => {
            // Hatchet is at-least-once: a delivery can remain in flight while
            // the local run is completed, cancelled or paused. A terminal
            // local decision must fence the executor before it reaches any
            // business side effect (especially a mail provider call).
            transaction.commit().await.map_err(HttpError)?;
            return Ok(StatusCode::ACCEPTED);
        }
        Ok(_)
        | Err(MaviError::NotFound {
            resource: "workflow_run",
        }) => {}
        Err(error) => return Err(HttpError(error)),
    }
    transaction.commit().await.map_err(HttpError)?;

    let idempotency_key = intent.idempotency_key.clone();
    if let Err(error) = executor.execute(intent).await {
        if matches!(
            &error,
            MaviError::Conflict { code } if code == "workflow_execution_in_progress"
        ) {
            // A second Hatchet delivery must not turn the first owner's
            // fenced execution into a failed projection. Preserve the
            // retryable response so Hatchet can redeliver after the lease.
            return Err(HttpError(error));
        }
        let mut transaction = runtime.begin(context).await.map_err(HttpError)?;
        match workflows
            .mark_run_failed(&mut transaction, &idempotency_key)
            .await
        {
            Ok(_)
            | Err(MaviError::NotFound {
                resource: "workflow_run",
            }) => {}
            Err(mark_error) => return Err(HttpError(mark_error)),
        }
        transaction.commit().await.map_err(HttpError)?;
        return Err(HttpError(error));
    }

    // Cron-triggered maintenance intents intentionally have no local outbox
    // row. Normal domain workflows always do, and are marked complete here
    // only after the Rust executor has committed its domain mutation.
    let mut transaction = runtime.begin(context).await.map_err(HttpError)?;
    match workflows.get_run(&mut transaction, &idempotency_key).await {
        // Some executors (for example a permanently rejected mail delivery)
        // finish with a successful transport response but a failed local
        // projection. Preserve that terminal failure instead of converting it
        // to completed in the generic HTTP wrapper.
        Ok(run) if run.status == "failed" => {}
        Ok(_) => match workflows
            .mark_completed(&mut transaction, &idempotency_key)
            .await
        {
            Ok(_)
            | Err(MaviError::NotFound {
                resource: "workflow_run",
            }) => {}
            Err(error) => return Err(HttpError(error)),
        },
        Err(MaviError::NotFound {
            resource: "workflow_run",
        }) => {}
        Err(error) => return Err(HttpError(error)),
    }
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::ACCEPTED)
}

/// Serves the stateless MCP HTTP transport defined by the current protocol
/// revision. Tool execution is translated back into the canonical HTTP
/// router, so authentication, Cedar and site admission are not duplicated in
/// an MCP-specific business-logic path.
async fn mcp_endpoint(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    headers: HeaderMap,
    Json(request): Json<McpRequest>,
) -> Result<Response, HttpError> {
    match context.caller {
        Caller::Account { .. } | Caller::Assistant { .. } => {}
        Caller::Public => return Err(HttpError(MaviError::Unauthenticated)),
        Caller::Student { .. } | Caller::System { .. } => {
            return Err(HttpError(MaviError::Forbidden));
        }
    }

    let id = request.id.clone();
    if request.jsonrpc != "2.0" {
        return Ok(mcp_error_response(
            id,
            -32600,
            "jsonrpc must be 2.0",
            StatusCode::BAD_REQUEST,
        ));
    }

    if headers
        .get(MCP_PROTOCOL_HEADER)
        .and_then(|value| value.to_str().ok())
        != Some(MCP_PROTOCOL_VERSION)
    {
        return Ok(mcp_error_response(
            id,
            -32022,
            "unsupported or missing MCP protocol version",
            StatusCode::BAD_REQUEST,
        ));
    }

    if headers
        .get(MCP_METHOD_HEADER)
        .and_then(|value| value.to_str().ok())
        != Some(request.method.as_str())
    {
        return Ok(mcp_error_response(
            id,
            -32020,
            "Mcp-Method does not match the JSON-RPC method",
            StatusCode::BAD_REQUEST,
        ));
    }

    if request.method.starts_with("notifications/") {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }

    match request.method.as_str() {
        "server/discover" => Ok(mcp_result_response(
            id,
            json!({
                "supportedVersions": [MCP_PROTOCOL_VERSION],
                "capabilities": {"tools": {"listChanged": false}},
                "instructions": "Use tools/list to discover site operations. Every tool call is site-scoped and Cedar-authorized.",
            }),
        )),
        "ping" => Ok(mcp_result_response(id, json!({}))),
        "tools/list" => mcp_tools_list(&state, &context, id, &request.params).await,
        "tools/call" => mcp_tool_call(&state, &context, &headers, id, &request.params).await,
        _ => Ok(mcp_error_response(
            id,
            -32601,
            "method not found",
            StatusCode::NOT_FOUND,
        )),
    }
}

async fn mcp_tool_call(
    state: &HttpState,
    context: &SiteContext,
    headers: &HeaderMap,
    id: Option<Value>,
    params: &Value,
) -> Result<Response, HttpError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| HttpError(MaviError::validation("mcp_tool_name_required")))?;
    if headers
        .get(MCP_NAME_HEADER)
        .and_then(|value| value.to_str().ok())
        != Some(name)
    {
        return Ok(mcp_error_response(
            id,
            -32020,
            "Mcp-Name does not match the requested tool",
            StatusCode::BAD_REQUEST,
        ));
    }

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let Some(arguments) = arguments.as_object() else {
        return Ok(mcp_error_response(
            id,
            -32602,
            "tool arguments must be an object",
            StatusCode::OK,
        ));
    };

    let catalog = active_api(state, context).await.map_err(HttpError)?;
    let active = active_plugins(state, context).await.map_err(HttpError)?;
    let Some(endpoint) = catalog
        .endpoints
        .iter()
        .find(|endpoint| endpoint.operation_id == name)
        .cloned()
    else {
        return Ok(mcp_error_response(
            id,
            -32602,
            "unknown tool",
            StatusCode::OK,
        ));
    };

    if !mcp_endpoint_available(state, context, &endpoint, &active) {
        return Ok(mcp_error_response(
            id,
            -32003,
            "tool is not available to this caller",
            StatusCode::OK,
        ));
    }

    let result = execute_mcp_tool(state, headers, &endpoint, arguments)
        .await
        .map_err(HttpError)?;
    Ok(mcp_result_response(id, result))
}

async fn mcp_tools_list(
    state: &HttpState,
    context: &SiteContext,
    id: Option<Value>,
    params: &Value,
) -> Result<Response, HttpError> {
    let cursor = params
        .get("cursor")
        .and_then(Value::as_str)
        .map(decode_mcp_cursor)
        .transpose()
        .map_err(HttpError)?
        .unwrap_or(0);
    let active = active_plugins(state, context).await.map_err(HttpError)?;
    let tools = available_mcp_tools(
        state,
        &active_api(state, context).await.map_err(HttpError)?,
        context,
        &active,
    )
    .map_err(HttpError)?;
    if cursor > tools.len() {
        return Ok(mcp_error_response(
            id,
            -32602,
            "invalid tools/list cursor",
            StatusCode::OK,
        ));
    }

    let end = cursor.saturating_add(MCP_TOOLS_PAGE_SIZE).min(tools.len());
    let mut result = json!({
        "tools": tools[cursor..end],
        "ttlMs": MCP_TOOLS_CACHE_TTL_MS,
        "cacheScope": "private",
    });
    if end < tools.len() {
        result["nextCursor"] = Value::String(encode_mcp_cursor(end));
    }
    Ok(mcp_result_response(id, result))
}

fn available_mcp_tools(
    state: &HttpState,
    catalog: &Api,
    context: &SiteContext,
    active: &BTreeSet<PluginId>,
) -> Result<Vec<Value>, MaviError> {
    let tools = catalog.mcp_tools().map_err(|_| MaviError::Internal)?["tools"]
        .as_array()
        .cloned()
        .ok_or(MaviError::Internal)?;
    let mut available = Vec::new();
    for tool in tools {
        let Some(endpoint) = catalog
            .endpoints
            .iter()
            .find(|endpoint| tool["name"].as_str() == Some(endpoint.operation_id.as_str()))
        else {
            continue;
        };
        if mcp_endpoint_available(state, context, endpoint, active) {
            available.push(tool);
        }
    }
    Ok(available)
}

fn mcp_endpoint_available(
    state: &HttpState,
    context: &SiteContext,
    endpoint: &Endpoint,
    active: &BTreeSet<PluginId>,
) -> bool {
    let caller_is_assistant = matches!(context.caller, Caller::Assistant { .. });
    let authentication_allows = match endpoint.authentication {
        mavi_contract::Authentication::AccountOrAssistant => true,
        mavi_contract::Authentication::Assistant => caller_is_assistant,
        _ => false,
    };
    if !authentication_allows {
        return false;
    }

    let Some(permission) = endpoint.permission.as_ref() else {
        return true;
    };
    let needed = permission.clone();
    state
        .authorization
        .authorize(
            context,
            &needed,
            "McpEndpoint",
            endpoint.operation_id.clone(),
            context.site_id,
            active,
        )
        .is_ok()
}

async fn execute_mcp_tool(
    state: &HttpState,
    headers: &HeaderMap,
    endpoint: &Endpoint,
    arguments: &serde_json::Map<String, Value>,
) -> Result<Value, MaviError> {
    let uri = mcp_uri(endpoint, arguments)?;
    let (body, content_type) = mcp_request_body(endpoint, arguments)?;
    let method = match endpoint.method {
        Method::Get => axum::http::Method::GET,
        Method::Post => axum::http::Method::POST,
        Method::Put => axum::http::Method::PUT,
        Method::Patch => axum::http::Method::PATCH,
        Method::Delete => axum::http::Method::DELETE,
    };

    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(value) = headers.get(AUTHORIZATION) {
        builder = builder.header(AUTHORIZATION, value);
    }
    if let Some(value) = headers.get(HOST) {
        builder = builder.header(HOST, value);
    }
    if let Some(content_type) = content_type {
        builder = builder.header(CONTENT_TYPE, content_type);
    }
    let request = builder.body(body).map_err(|_| MaviError::Internal)?;
    let dispatcher = state.mcp_dispatcher.get().ok_or(MaviError::Internal)?;
    let response = dispatcher
        .clone()
        .oneshot(request)
        .await
        .map_err(|_| MaviError::Internal)?;
    let successful = response.status().is_success();
    let is_json = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    let bytes = to_bytes(response.into_body(), MAX_FILE_BYTES + 1)
        .await
        .map_err(|_| MaviError::Internal)?;

    let (text, structured) = if is_json {
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| MaviError::Internal)?;
        let text = serde_json::to_string(&value).map_err(|_| MaviError::Internal)?;
        let structured = value.is_object().then_some(value);
        (text, structured)
    } else {
        (
            base64::engine::general_purpose::STANDARD.encode(bytes),
            None,
        )
    };

    let mut result = json!({
        "content": [{"type": "text", "text": text}],
        "isError": !successful,
    });
    if let Some(structured) = structured {
        result["structuredContent"] = structured;
    }
    Ok(result)
}

fn mcp_uri(
    endpoint: &Endpoint,
    arguments: &serde_json::Map<String, Value>,
) -> Result<String, MaviError> {
    let path_arguments = arguments.get("path").and_then(Value::as_object);
    let mut rendered = String::with_capacity(endpoint.path.len());
    let mut rest = endpoint.path.as_str();
    while let Some(open) = rest.find('{') {
        rendered.push_str(&rest[..open]);
        let after_open = &rest[open + 1..];
        let close = after_open
            .find('}')
            .ok_or_else(|| MaviError::validation("invalid_mcp_operation_path"))?;
        let name = &after_open[..close];
        let value = path_arguments
            .and_then(|values| values.get(name))
            .and_then(mcp_scalar_string)
            .ok_or_else(|| MaviError::validation_field("mcp_path_parameter_required", name))?;
        rendered.push_str(&percent_encode(&value));
        rest = &after_open[close + 1..];
    }
    rendered.push_str(rest);

    let query = endpoint
        .request
        .as_ref()
        .filter(|request| request.location == InputLocation::Query)
        .map(|_| arguments.get("query"))
        .or_else(|| endpoint.query.as_ref().map(|_| arguments.get("query")))
        .flatten();
    if let Some(query) = query {
        let Some(values) = query.as_object() else {
            return Err(MaviError::validation("mcp_query_must_be_object"));
        };
        let mut pairs = Vec::new();
        let mut keys = values.keys().collect::<Vec<_>>();
        keys.sort();
        for key in keys {
            let value = &values[key];
            match value {
                Value::Null => {}
                Value::Array(items) => {
                    for item in items {
                        if let Some(value) = mcp_scalar_string(item) {
                            pairs.push(format!(
                                "{}={}",
                                percent_encode(key),
                                percent_encode(&value)
                            ));
                        }
                    }
                }
                value => {
                    let value = mcp_scalar_string(value)
                        .ok_or_else(|| MaviError::validation("mcp_query_value_invalid"))?;
                    pairs.push(format!(
                        "{}={}",
                        percent_encode(key),
                        percent_encode(&value)
                    ));
                }
            }
        }
        if !pairs.is_empty() {
            rendered.push('?');
            rendered.push_str(&pairs.join("&"));
        }
    }
    Ok(rendered)
}

fn mcp_request_body(
    endpoint: &Endpoint,
    arguments: &serde_json::Map<String, Value>,
) -> Result<(Body, Option<&'static str>), MaviError> {
    let Some(request) = endpoint.request.as_ref() else {
        return Ok((Body::empty(), None));
    };
    match request.location {
        InputLocation::Query => Ok((Body::empty(), None)),
        InputLocation::Json => {
            let value = arguments
                .get("body")
                .ok_or_else(|| MaviError::validation("mcp_body_required"))?;
            let body = serde_json::to_vec(value).map_err(|_| MaviError::Internal)?;
            Ok((Body::from(body), Some("application/json")))
        }
        InputLocation::Raw => {
            let encoded = arguments
                .get("body")
                .and_then(Value::as_str)
                .ok_or_else(|| MaviError::validation("mcp_raw_body_base64_required"))?;
            let body = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| MaviError::validation("mcp_raw_body_base64_invalid"))?;
            Ok((Body::from(body), Some("application/octet-stream")))
        }
    }
}

fn mcp_scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            write!(encoded, "{byte:02X}").expect("writing to a String cannot fail");
        }
    }
    encoded
}

fn encode_mcp_cursor(index: usize) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(index.to_string())
}

fn decode_mcp_cursor(cursor: &str) -> Result<usize, MaviError> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| MaviError::validation("invalid_mcp_cursor"))?;
    String::from_utf8(bytes)
        .map_err(|_| MaviError::validation("invalid_mcp_cursor"))?
        .parse::<usize>()
        .map_err(|_| MaviError::validation("invalid_mcp_cursor"))
}

fn mcp_result_response(id: Option<Value>, mut result: Value) -> Response {
    if let Some(object) = result.as_object_mut() {
        object.insert(
            "_meta".to_owned(),
            json!({MCP_SERVER_INFO_META: {"name": "mavi", "version": env!("CARGO_PKG_VERSION")}}),
        );
    }
    (
        StatusCode::OK,
        Json(json!({"jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": result})),
    )
        .into_response()
}

fn mcp_error_response(
    id: Option<Value>,
    code: i32,
    message: &'static str,
    status: StatusCode,
) -> Response {
    (
        status,
        Json(json!({
            "jsonrpc": "2.0",
            "id": id.unwrap_or(Value::Null),
            "error": {"code": code, "message": message}
        })),
    )
        .into_response()
}

struct HttpState {
    runtime: SiteRuntime,
    plugins: PluginService,
    authorization: AuthorizationService,
    workflows: WorkflowService,
    identity: IdentityService,
    content: ContentService,
    settings: SettingsService,
    taxonomy: TaxonomyService,
    media: MediaService,
    audit: AuditService,
    trash: TrashService,
    design: DesignService,
    forms: FormService,
    feedback: FeedbackService,
    mail: MailService,
    shop: ShopService,
    courses: CoursesService,
    jobs: WorkflowScheduler,
    flows: FlowService,
    boards: BoardService,
    analytics: AnalyticsService,
    portable: PortableService,
    credentials: CredentialService,
    metrics: RuntimeMetrics,
    file_store: Arc<dyn FileStore>,
    builder: Arc<dyn BuildEngine>,
    sealer: Arc<dyn Seals>,
    edge: EdgeSecurityConfig,
    mcp_dispatcher: Arc<OnceLock<Router>>,
    mail_webhook_token: Option<Arc<str>>,
    hatchet_bridge: Option<HatchetBridgeClient>,
    bridge_secret: Option<Arc<str>>,
    workflow_executor: Option<Arc<dyn WorkflowExecutor>>,
}

impl Clone for HttpState {
    fn clone(&self) -> Self {
        Self {
            runtime: self.runtime.clone(),
            plugins: self.plugins.clone(),
            authorization: self.authorization.clone(),
            workflows: self.workflows.clone(),
            identity: self.identity,
            content: self.content,
            settings: self.settings,
            taxonomy: self.taxonomy,
            media: self.media,
            audit: self.audit,
            trash: self.trash,
            design: self.design,
            forms: self.forms,
            feedback: self.feedback,
            mail: self.mail,
            shop: self.shop,
            courses: self.courses,
            jobs: self.jobs.clone(),
            flows: self.flows,
            boards: self.boards,
            analytics: self.analytics,
            portable: self.portable,
            credentials: self.credentials,
            metrics: self.metrics.clone(),
            file_store: Arc::clone(&self.file_store),
            builder: Arc::clone(&self.builder),
            sealer: Arc::clone(&self.sealer),
            edge: self.edge.clone(),
            mcp_dispatcher: Arc::clone(&self.mcp_dispatcher),
            mail_webhook_token: self.mail_webhook_token.clone(),
            hatchet_bridge: self.hatchet_bridge.clone(),
            bridge_secret: self.bridge_secret.clone(),
            workflow_executor: self.workflow_executor.clone(),
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

async fn admit(
    State(runtime): State<SiteRuntime>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .copied()
        .unwrap_or_else(RequestId::new);
    let request_id_header = HeaderValue::from_str(&request_id.to_string())
        .expect("UUID request IDs are always valid header values");

    let response = match runtime.context(request_id) {
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

async fn authenticate(
    State(state): State<HttpState>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let token = match authorization_token(&request) {
        Ok(Some(token)) => token,
        Ok(None) => return next.run(request).await,
        Err(error) => return HttpError(error).into_response(),
    };
    let Some(public_context) = request.extensions().get::<SiteContext>().cloned() else {
        return HttpError(MaviError::Internal).into_response();
    };

    if request.uri().path() == MAIL_PROVIDER_EVENTS_PATH {
        let Some(expected) = state.mail_webhook_token.as_deref() else {
            return HttpError(MaviError::Unauthenticated).into_response();
        };
        if !secrets_equal(expected, token) {
            return HttpError(MaviError::Unauthenticated).into_response();
        }
        request.extensions_mut().insert(SiteContext::system(
            public_context.site_id,
            "mail-webhook",
            public_context.request_id,
        ));
        return next.run(request).await;
    }

    let mut transaction = match state.runtime.begin(&public_context).await {
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

fn secrets_equal(expected: &str, presented: &str) -> bool {
    let expected = expected.as_bytes();
    let presented = presented.as_bytes();
    let mut difference = (expected.len() ^ presented.len()) as u64;
    for index in 0..expected.len().max(presented.len()) {
        difference |= u64::from(
            expected.get(index).copied().unwrap_or_default()
                ^ presented.get(index).copied().unwrap_or_default(),
        );
    }
    difference == 0
}

async fn edge_throttle(
    State(state): State<HttpState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let Some(action) = edge::action_for(&request) else {
        return next.run(request).await;
    };
    let Some(context) = request.extensions().get::<SiteContext>() else {
        return HttpError(MaviError::Internal).into_response();
    };
    let source = edge::source_for(&request, &state.edge.trusted_proxies);
    let decision =
        match state
            .edge
            .limiter()
            .check(context.site_id, action, source, std::time::Instant::now())
        {
            Ok(decision) => decision,
            Err(error) => return HttpError(error).into_response(),
        };

    let Some(scope) = decision.limited_scope else {
        return next.run(request).await;
    };

    if decision.audit_required {
        let fingerprint = decision.fingerprint.map(edge::fingerprint_text);
        if let Err(error) = record_edge_throttle(&state, context, action, scope, fingerprint).await
        {
            return HttpError(error).into_response();
        }
    }

    let mut response = HttpError(MaviError::RateLimited).into_response();
    if let Ok(value) = HeaderValue::from_str(&decision.retry_after_seconds.to_string()) {
        response
            .headers_mut()
            .insert(axum::http::header::RETRY_AFTER, value);
    }
    response
}

async fn record_edge_throttle(
    state: &HttpState,
    context: &SiteContext,
    action: EdgeAction,
    scope: edge::ThrottleScope,
    fingerprint: Option<String>,
) -> Result<(), MaviError> {
    let audit_action = match action {
        EdgeAction::FormSubmissionCreate => forms_audit_action::SECURITY_EDGE_RATE_LIMITED,
        _ => audit_action::SECURITY_EDGE_RATE_LIMITED,
    };
    let mut transaction = state.runtime.begin(context).await?;
    AuditService
        .record(
            &mut transaction,
            context,
            &AuditEntry {
                action: audit_action.to_owned(),
                resource_type: "Site".to_owned(),
                resource_id: Some(context.site_id.into_uuid()),
                payload: json!({
                    "action": action.as_str(),
                    "scope": scope.as_str(),
                    "fingerprint": fingerprint,
                }),
            },
        )
        .await?;
    transaction.commit().await
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

fn require_automation_grant(
    state: &HttpState,
    context: &SiteContext,
    action: Action,
    resource_type: impl Into<String>,
    resource_id: impl Into<String>,
) -> Result<(), HttpError> {
    require_grant_for(
        state,
        context,
        Grant::new(Capability::Automation, action),
        resource_type,
        resource_id,
    )
}

fn require_workflow_permission(
    state: &HttpState,
    context: &SiteContext,
    action: &str,
    resource_type: impl Into<String>,
    resource_id: impl Into<String>,
) -> Result<(), HttpError> {
    let permission = BusinessPermission::new(PluginId::Automation, action);
    require_permission_for(state, context, &permission, resource_type, resource_id)
}

fn require_boards_grant(
    state: &HttpState,
    context: &SiteContext,
    action: Action,
    resource_type: impl Into<String>,
    resource_id: impl Into<String>,
) -> Result<(), HttpError> {
    require_grant_for(
        state,
        context,
        Grant::new(Capability::Boards, action),
        resource_type,
        resource_id,
    )
}

fn require_analytics_grant(
    state: &HttpState,
    context: &SiteContext,
    action: Action,
    resource_type: impl Into<String>,
    resource_id: impl Into<String>,
) -> Result<(), HttpError> {
    require_grant_for(
        state,
        context,
        Grant::new(Capability::Analytics, action),
        resource_type,
        resource_id,
    )
}

fn require_portable_grant(
    state: &HttpState,
    context: &SiteContext,
    action: Action,
    resource_type: impl Into<String>,
    resource_id: impl Into<String>,
) -> Result<(), HttpError> {
    require_grant_for(
        state,
        context,
        Grant::new(Capability::Portable, action),
        resource_type,
        resource_id,
    )
}

fn require_credentials_grant(
    state: &HttpState,
    context: &SiteContext,
    action: Action,
    resource_id: impl Into<String>,
) -> Result<(), HttpError> {
    require_grant_for(
        state,
        context,
        Grant::new(Capability::Credentials, action),
        "Credential",
        resource_id,
    )
}

fn require_courses_grant(
    state: &HttpState,
    context: &SiteContext,
    action: Action,
    resource_type: impl Into<String>,
    resource_id: impl Into<String>,
) -> Result<(), HttpError> {
    require_grant_for(
        state,
        context,
        Grant::new(Capability::Courses, action),
        resource_type,
        resource_id,
    )
}

async fn require_course_grant(
    state: &HttpState,
    context: &SiteContext,
    transaction: &mut SiteTx,
    action: Action,
    course_id: CourseId,
    resource_type: impl Into<String>,
    resource_id: impl Into<String>,
) -> Result<(), HttpError> {
    let grant = Grant::new(Capability::Courses, action);
    let resource_grants = state
        .courses
        .instructor_grants(transaction, context, course_id)
        .await
        .map_err(HttpError)?;
    require_grant_for_with_resource_grants(
        state,
        context,
        grant,
        resource_type,
        resource_id,
        &resource_grants,
    )
}

fn require_grant(
    state: &HttpState,
    context: &SiteContext,
    grant: Grant,
    resource_id: impl Into<String>,
) -> Result<(), HttpError> {
    require_grant_for(state, context, grant, "Content", resource_id)
}

fn require_grant_for(
    state: &HttpState,
    context: &SiteContext,
    grant: Grant,
    resource_type: impl Into<String>,
    resource_id: impl Into<String>,
) -> Result<(), HttpError> {
    state
        .authorization
        .authorize_grant(
            context,
            grant,
            resource_type,
            resource_id,
            context.site_id,
            &Grants::default(),
        )
        .map_err(HttpError)
}

fn require_permission_for(
    state: &HttpState,
    context: &SiteContext,
    permission: &BusinessPermission,
    resource_type: impl Into<String>,
    resource_id: impl Into<String>,
) -> Result<(), HttpError> {
    state
        .authorization
        .authorize_cached(
            context,
            permission,
            resource_type,
            resource_id,
            context.site_id,
        )
        .map_err(HttpError)
}

fn require_grant_for_with_resource_grants(
    state: &HttpState,
    context: &SiteContext,
    grant: Grant,
    resource_type: impl Into<String>,
    resource_id: impl Into<String>,
    resource_grants: &Grants,
) -> Result<(), HttpError> {
    state
        .authorization
        .authorize_grant(
            context,
            grant,
            resource_type,
            resource_id,
            context.site_id,
            resource_grants,
        )
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
    fn webhook_secret_comparison_requires_equal_bytes_and_length() {
        assert!(secrets_equal("provider-secret", "provider-secret"));
        assert!(!secrets_equal("provider-secret", "provider-secret-extra"));
        assert!(!secrets_equal("provider-secret", "provider-secrex"));
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
        assert!(catalog.endpoints.iter().any(|endpoint| {
            endpoint.operation_id == "mail.provider_events.receive"
                && endpoint.authentication == mavi_contract::Authentication::Webhook
        }));
    }

    #[test]
    fn plugin_gate_uses_canonical_endpoint_templates() {
        assert!(endpoint_path_matches(
            "/api/v1/shop/products/{id}",
            "/api/v1/shop/products/product-1"
        ));
        assert!(!endpoint_path_matches(
            "/api/v1/shop/products/{id}",
            "/api/v1/shop/products/product-1/variants"
        ));

        let catalog = api();
        let shop = catalog
            .endpoints
            .iter()
            .find(|endpoint| endpoint.operation_id == "shop.products.list")
            .expect("shop endpoint");
        assert_eq!(shop.required_plugin, PluginId::Commerce);

        let content_trash = catalog
            .endpoints
            .iter()
            .find(|endpoint| endpoint.operation_id == "content.trash")
            .expect("content trash endpoint");
        assert_eq!(content_trash.required_plugin, PluginId::Governance);
        let content_restore = catalog
            .endpoints
            .iter()
            .find(|endpoint| endpoint.operation_id == "content.restore")
            .expect("content restore endpoint");
        assert_eq!(content_restore.required_plugin, PluginId::Governance);
    }

    #[test]
    fn runtime_contract_filters_disabled_plugins() {
        let active = [PluginId::Core, PluginId::Writing].into_iter().collect();
        let catalog = api().for_plugins(&active);
        assert!(
            catalog
                .endpoints
                .iter()
                .all(|endpoint| active.contains(&endpoint.required_plugin))
        );
        assert!(
            !catalog
                .endpoints
                .iter()
                .any(|endpoint| endpoint.operation_id == "shop.products.list")
        );
        assert!(
            catalog
                .endpoints
                .iter()
                .any(|endpoint| endpoint.operation_id == "content.list")
        );
    }

    #[tokio::test]
    async fn json_boundary_rejects_unknown_fields_with_a_typed_error() {
        #[derive(Debug, Deserialize)]
        #[allow(dead_code)]
        struct Input {
            value: String,
        }

        let request = Request::builder()
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"value":"ok","typo":true}"#))
            .expect("request");
        let response = Json::<Input>::from_request(request, &())
            .await
            .expect_err("unknown JSON field");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 4096)
            .await
            .expect("error body");
        let error: Value = serde_json::from_slice(&body).expect("JSON error");
        assert_eq!(error["error"]["code"], "unknown_field");
        assert_eq!(error["error"]["field"], "typo");
    }

    #[tokio::test]
    async fn query_boundary_rejects_unknown_fields_with_a_typed_error() {
        #[derive(Debug, Deserialize)]
        #[allow(dead_code)]
        struct Filter {
            limit: Option<u32>,
        }

        let request = Request::builder()
            .uri("/api/v1/content?limit=10&typo=1")
            .body(Body::empty())
            .expect("request");
        let (mut parts, _) = request.into_parts();
        let response = Query::<Filter>::from_request_parts(&mut parts, &())
            .await
            .expect_err("unknown query field");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 4096)
            .await
            .expect("error body");
        let error: Value = serde_json::from_slice(&body).expect("JSON error");
        assert_eq!(error["error"]["code"], "unknown_field");
        assert_eq!(error["error"]["field"], "typo");
    }
}
