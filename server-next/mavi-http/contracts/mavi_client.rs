// Generated from the canonical Mavi API. Do not edit by hand.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Activity {
    pub id: String,
    pub board_id: String,
    pub card_id: Option<String>,
    pub kind: String,
    pub actor_kind: String,
    pub actor_id: Option<String>,
    pub detail: Value,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActivityPage {
    pub items: Vec<Activity>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActivityPageFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub card_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AddReader {
    pub email: String,
    pub name: Option<String>,
    pub resubscribe: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnalyticsEvent {
    pub id: String,
    pub event_name: String,
    pub path: String,
    pub value: i64,
    pub occurred_at: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnalyticsEventBatch {
    pub events: Vec<AnalyticsEventInput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnalyticsEventInput {
    pub event_name: String,
    pub path: String,
    pub value: Option<i64>,
    pub occurred_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnalyticsEventPage {
    pub items: Vec<AnalyticsEvent>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnalyticsReceipt {
    pub accepted: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiKeyCreated {
    pub id: String,
    pub site_id: String,
    pub person_id: String,
    pub name: String,
    pub prefix: String,
    pub token: String,
    pub grants: Vec<Grant>,
    pub expires_at: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiKeyListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub revoked: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiKeyPage {
    pub items: Vec<ApiKeyRecord>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiKeyRecord {
    pub id: String,
    pub site_id: String,
    pub person_id: String,
    pub name: String,
    pub prefix: String,
    pub grants: Vec<Grant>,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AssignCard {
    pub assignee_id: Option<String>,
}

pub type AuditActorKind = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuditEvent {
    pub id: String,
    pub request_id: String,
    pub actor_kind: AuditActorKind,
    pub actor_id: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuditEventPage {
    pub items: Vec<AuditEvent>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuditListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub actor_kind: Option<AuditActorKind>,
    pub actor_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BasketItem {
    pub product_id: String,
    pub quantity: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Board {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BoardList {
    pub id: String,
    pub board_id: String,
    pub name: String,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BoardListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub archived: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BoardListPage {
    pub items: Vec<BoardList>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BoardPage {
    pub items: Vec<Board>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Card {
    pub id: String,
    pub board_id: String,
    pub list_id: String,
    pub title: String,
    pub description: Option<String>,
    pub assignee_id: Option<String>,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CardPage {
    pub items: Vec<Card>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CardPageFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub assignee_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CheckoutInput {
    pub email: String,
    pub items: Vec<BasketItem>,
    pub coupon_code: Option<String>,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CheckoutReceipt {
    pub id: String,
    pub number: i64,
    pub state: OrderState,
    pub total: Money,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Comment {
    pub id: String,
    pub board_id: String,
    pub card_id: String,
    pub author_id: Option<String>,
    pub body: String,
    pub edited_at: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CommentPage {
    pub items: Vec<Comment>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CommentPageFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Content {
    pub id: String,
    pub site_id: String,
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub body: String,
    pub fields: Value,
    pub publication: Publication,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub type ContentFieldKind = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub kind: Option<String>,
    pub language: Option<String>,
    pub status: Option<PublicationStatus>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentPage {
    pub items: Vec<Content>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentRevision {
    pub content_id: String,
    pub revision: i64,
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub body: String,
    pub fields: Value,
    pub publication: Publication,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentRevisionListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentRevisionPage {
    pub items: Vec<ContentRevision>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentTermAssignment {
    pub content_id: String,
    pub assigned_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentTermAssignmentListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentTermAssignmentPage {
    pub items: Vec<ContentTermAssignment>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentType {
    pub site_id: String,
    pub kind: String,
    pub name: String,
    pub fields: Vec<ContentTypeField>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentTypeField {
    pub key: String,
    pub label: String,
    pub required: bool,
    pub kind: ContentFieldKind,
    pub options: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentTypeListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentTypePage {
    pub items: Vec<ContentType>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Coupon {
    pub id: String,
    pub code: String,
    pub kind: CouponKind,
    pub percent: Option<i64>,
    pub amount: Value,
    pub max_uses: Option<i64>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub type CouponKind = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CouponListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CouponPage {
    pub items: Vec<Coupon>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Course {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub about: Option<String>,
    pub state: CourseState,
    pub modules: Vec<Module>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CourseListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub state: Option<CourseState>,
}

pub type CourseState = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CourseSummary {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub about: Option<String>,
    pub state: CourseState,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CourseSummaryPage {
    pub items: Vec<CourseSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateApiKey {
    pub name: String,
    pub grants: Vec<Grant>,
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateBoard {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateCard {
    pub title: String,
    pub description: Option<String>,
    pub assignee_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateComment {
    pub body: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateContent {
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub body: Option<String>,
    pub fields: Option<Value>,
    pub publication: Option<PublicationInput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateCoupon {
    pub code: String,
    pub percent: Option<i64>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub max_uses: Option<i64>,
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateCourse {
    pub slug: String,
    pub title: String,
    pub about: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateCredential {
    pub provider: String,
    pub name: String,
    pub values: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateFlow {
    pub name: String,
    pub trigger: Trigger,
    pub steps: Vec<FlowStepInput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateForm {
    pub slug: String,
    pub name: String,
    pub fields: Option<Vec<FormField>>,
    pub kept_days: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateLanguage {
    pub tag: String,
    pub name: String,
    pub is_default: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateLesson {
    pub title: String,
    pub body: Option<String>,
    pub media_file_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateList {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateMailList {
    pub slug: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateMailTemplate {
    pub key: String,
    pub language: String,
    pub subject: String,
    pub body: String,
    pub content_type: Option<MailContentType>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateModule {
    pub title: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreatePerson {
    pub email: String,
    pub name: String,
    pub password: String,
    pub role_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateProduct {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub price: ProductPrice,
    pub stock: i64,
    pub on_sale: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateRole {
    pub name: String,
    pub grants: Option<Vec<Grant>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateStudent {
    pub email: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateTerm {
    pub kind: TermKind,
    pub language: String,
    pub slug: String,
    pub name: String,
    pub parent_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Credential {
    pub id: String,
    pub site_id: String,
    pub provider: String,
    pub name: String,
    pub state: String,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CredentialListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CredentialPage {
    pub items: Vec<Credential>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DailyAggregate {
    pub day: String,
    pub event_name: String,
    pub path: String,
    pub event_count: i64,
    pub value_sum: i64,
    pub value_min: i64,
    pub value_max: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DailyAggregatePage {
    pub items: Vec<DailyAggregate>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DailyListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub event_name: Option<String>,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeclareContentType {
    pub name: String,
    pub fields: Option<Vec<ContentTypeField>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeliveryListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub status: Option<MailDeliveryStatus>,
}

pub type DesignAsset = Vec<u8>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DesignBuild {
    pub id: String,
    pub change_id: String,
    pub state: DesignBuildState,
    pub error: Option<String>,
    pub preview_path: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DesignBuildListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DesignBuildPage {
    pub items: Vec<DesignBuild>,
    pub next_cursor: Option<String>,
}

pub type DesignBuildState = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DesignChange {
    pub id: String,
    pub name: String,
    pub state: DesignState,
    pub ready_build_id: Option<String>,
    pub published_build_id: Option<String>,
    pub last_error: Option<String>,
    pub published_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DesignChangeListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub state: Option<DesignState>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DesignChangePage {
    pub items: Vec<DesignChange>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DesignFile {
    pub path: String,
    pub contents: String,
    pub bytes: i64,
    pub sha256: String,
    pub removed: bool,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DesignFileInput {
    pub path: String,
    pub contents: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DesignFileListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DesignFilePage {
    pub items: Vec<DesignFileSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DesignFileQuery {
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DesignFileSummary {
    pub path: String,
    pub bytes: i64,
    pub sha256: String,
    pub removed: bool,
    pub updated_at: String,
}

pub type DesignState = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EmailVerificationRedeem {
    pub token: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EmailVerificationRequest {
    pub email: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EmailVerificationRequested {
    pub accepted: bool,
}

pub type Empty = Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EnqueueDelivery {
    pub recipient: String,
    pub template_id: String,
    pub variables: Option<Value>,
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EnrollStudent {
    pub student_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Enrollment {
    pub id: String,
    pub course_id: String,
    pub student_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EnrollmentListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EnrollmentPage {
    pub items: Vec<Enrollment>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub field: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EventListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub event_name: Option<String>,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct File {
    pub id: String,
    pub kind: FileKind,
    pub visibility: FileVisibility,
    pub mime: String,
    pub name: String,
    pub bytes: i64,
    pub sha256: String,
    pub created_at: String,
}

pub type FileBytes = Vec<u8>;

pub type FileKind = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FileListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub kind: Option<FileKind>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FilePage {
    pub items: Vec<File>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FileVariant {
    pub id: String,
    pub source_file_id: String,
    pub preset: VariantPreset,
    pub mime: String,
    pub width: i64,
    pub height: i64,
    pub bytes: i64,
    pub sha256: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FileVariantListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FileVariantPage {
    pub items: Vec<FileVariant>,
    pub next_cursor: Option<String>,
}

pub type FileVisibility = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Flow {
    pub id: String,
    pub name: String,
    pub trigger: Trigger,
    pub enabled: bool,
    pub version: i64,
    pub steps: Vec<FlowStep>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FlowListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub trigger: Option<Value>,
    pub enabled: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FlowPage {
    pub items: Vec<Flow>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FlowRun {
    pub id: String,
    pub flow_id: String,
    pub trigger: Trigger,
    pub event: Value,
    pub definition: Vec<FlowStepInput>,
    pub state: RunState,
    pub current_position: i64,
    pub retry_count: i64,
    pub last_error: Option<String>,
    pub steps: Vec<FlowRunStep>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FlowRunPage {
    pub items: Vec<FlowRun>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FlowRunStep {
    pub id: String,
    pub position: i64,
    pub attempt: i64,
    pub kind: StepKind,
    pub outcome: String,
    pub detail: Value,
    pub error: Option<String>,
    pub started_at: String,
    pub finished_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FlowStep {
    pub id: String,
    pub position: i64,
    pub kind: StepKind,
    pub config: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FlowStepInput {
    pub kind: StepKind,
    pub config: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Form {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub fields: Vec<FormField>,
    pub open: bool,
    pub kept_days: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FormExportMetadata {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub fields: Vec<FormField>,
    pub kept_days: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FormField {
    pub key: String,
    pub label: String,
    pub required: bool,
    pub kind: FormFieldKind,
    pub options: Vec<String>,
}

pub type FormFieldKind = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FormListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FormPage {
    pub items: Vec<Form>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FormSubmission {
    pub id: String,
    pub form_id: String,
    pub answers: Value,
    pub seen_at: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FormSubmissionExport {
    pub format: String,
    pub version: i64,
    pub form: FormExportMetadata,
    pub items: Vec<FormSubmission>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Grant {
    pub capability: String,
    pub action: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ImportReceipt {
    pub strategy: ImportStrategy,
    pub languages: i64,
    pub content_types: i64,
    pub terms: i64,
    pub content: i64,
    pub revisions: i64,
    pub slug_history: i64,
    pub assignments: i64,
}

pub type ImportStrategy = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub payload: Value,
    pub state: JobState,
    pub run_at: String,
    pub claimed_until: Option<String>,
    pub claimed_by: Option<String>,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub idempotency_key: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JobListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub state: Option<JobState>,
    pub kind: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JobPage {
    pub items: Vec<Job>,
    pub next_cursor: Option<String>,
}

pub type JobState = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Language {
    pub site_id: String,
    pub tag: String,
    pub name: String,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LanguageListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LanguagePage {
    pub items: Vec<Language>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LearningCourse {
    pub course_id: String,
    pub slug: String,
    pub title: String,
    pub about: Option<String>,
    pub state: CourseState,
    pub completed_lessons: i64,
    pub total_lessons: i64,
    pub enrolled_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LearningCourseListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LearningCoursePage {
    pub items: Vec<LearningCourse>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LearningLesson {
    pub lesson: Lesson,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Lesson {
    pub id: String,
    pub module_id: String,
    pub title: String,
    pub body: String,
    pub media_file_id: Option<String>,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LessonListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LessonPage {
    pub items: Vec<Lesson>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ListPageFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

pub type MailContentType = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MailDelivery {
    pub id: String,
    pub template_id: Option<String>,
    pub list_id: Option<String>,
    pub recipient: String,
    pub subject: String,
    pub body: String,
    pub body_protected: bool,
    pub content_type: MailContentType,
    pub purpose: MailPurpose,
    pub status: MailDeliveryStatus,
    pub attempts: i64,
    pub available_at: String,
    pub provider: Option<String>,
    pub provider_reference: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub sent_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MailDeliveryPage {
    pub items: Vec<MailDelivery>,
    pub next_cursor: Option<String>,
}

pub type MailDeliveryStatus = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MailList {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub subscriber_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MailListListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MailListPage {
    pub items: Vec<MailList>,
    pub next_cursor: Option<String>,
}

pub type MailPurpose = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MailReader {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub standing: MailStanding,
    pub added_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MailReaderCreated {
    pub reader: MailReader,
    pub unsubscribe_token: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MailReaderPage {
    pub items: Vec<MailReader>,
    pub next_cursor: Option<String>,
}

pub type MailStanding = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MailTemplate {
    pub id: String,
    pub key: String,
    pub language: String,
    pub subject: String,
    pub body: String,
    pub content_type: MailContentType,
    pub variables: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MailTemplateListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MailTemplatePage {
    pub items: Vec<MailTemplate>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MailTemplatePreview {
    pub variables: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Module {
    pub id: String,
    pub course_id: String,
    pub title: String,
    pub position: i64,
    pub lessons: Vec<Lesson>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Money {
    pub minor: i64,
    pub currency: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MoveCard {
    pub list_id: String,
    pub before_card_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Order {
    pub id: String,
    pub number: i64,
    pub state: OrderState,
    pub email: String,
    pub total: Money,
    pub lines: Vec<OrderLine>,
    pub payment_provider: Option<String>,
    pub payment_reference: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OrderLine {
    pub id: String,
    pub product_id: Option<String>,
    pub name: String,
    pub each: Money,
    pub quantity: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OrderListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub state: Option<OrderState>,
}

pub type OrderState = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OrderSummary {
    pub id: String,
    pub number: i64,
    pub state: OrderState,
    pub email: String,
    pub total: Money,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OrderSummaryPage {
    pub items: Vec<OrderSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OrderTransition {
    pub to: OrderState,
    pub payment: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaginationContract {
    pub style: String,
    pub default_limit: i64,
    pub max_limit: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PasswordResetRedeem {
    pub token: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PasswordResetRequest {
    pub email: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PasswordResetRequested {
    pub accepted: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentReceiptInput {
    pub provider: String,
    pub reference: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PeopleListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub status: Option<PersonListFilterStatus>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Person {
    pub id: String,
    pub site_id: String,
    pub email: String,
    pub name: String,
    pub email_verified: bool,
}

pub type PersonListFilterStatus = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PersonPage {
    pub items: Vec<PersonRecord>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PersonRecord {
    pub id: String,
    pub site_id: String,
    pub email: String,
    pub name: String,
    pub status: PersonListFilterStatus,
    pub email_verified: bool,
    pub role_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PortableAssignment {
    pub content_id: String,
    pub term_id: String,
    pub assigned_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PortableBundle {
    pub manifest: PortableManifest,
    pub site: PortableSite,
    pub languages: Vec<PortableLanguage>,
    pub content_types: Vec<PortableContentType>,
    pub terms: Vec<PortableTerm>,
    pub content: Vec<PortableContent>,
    pub revisions: Vec<PortableRevision>,
    pub slug_history: Vec<PortableSlugHistory>,
    pub assignments: Vec<PortableAssignment>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PortableContent {
    pub id: String,
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub body: String,
    pub fields: Value,
    pub status: String,
    pub scheduled_at: Option<String>,
    pub published_at: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PortableContentType {
    pub kind: String,
    pub name: String,
    pub fields: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PortableCounts {
    pub languages: i64,
    pub content_types: i64,
    pub terms: i64,
    pub content: i64,
    pub revisions: i64,
    pub slug_history: i64,
    pub assignments: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PortableImportRequest {
    pub bundle: PortableBundle,
    pub strategy: ImportStrategy,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PortableLanguage {
    pub tag: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PortableManifest {
    pub format: String,
    pub version: i64,
    pub source_site_id: String,
    pub exported_at: String,
    pub schema_hash: String,
    pub counts: PortableCounts,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PortableRevision {
    pub content_id: String,
    pub revision: i64,
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub body: String,
    pub fields: Value,
    pub status: String,
    pub scheduled_at: Option<String>,
    pub published_at: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PortableSite {
    pub name: String,
    pub timezone: String,
    pub canonical_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PortableSlugHistory {
    pub content_id: String,
    pub language: String,
    pub slug: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PortableTerm {
    pub id: String,
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub name: String,
    pub parent_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Product {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub price: Money,
    pub stock: i64,
    pub on_sale: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProductListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProductPage {
    pub items: Vec<Product>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProductPrice {
    pub minor: i64,
    pub currency: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Progress {
    pub lesson_id: String,
    pub completed_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PruneAnalytics {
    pub raw_days: i64,
    pub aggregate_days: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PruneReceipt {
    pub deleted_events: i64,
    pub deleted_aggregates: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PublicContentQuery {
    pub language: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PublicForm {
    pub slug: String,
    pub name: String,
    pub fields: Vec<FormField>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PublicProduct {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub price: Money,
    pub can_be_bought: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PublicProductListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PublicProductPage {
    pub items: Vec<PublicProduct>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PublicTermArchiveQuery {
    pub language: Option<String>,
    pub after: Option<String>,
    pub limit: Option<i64>,
}

pub type Publication = Value;

pub type PublicationInput = Value;

pub type PublicationStatus = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReaderListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub standing: Option<MailStanding>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenderedMail {
    pub subject: String,
    pub body: String,
    pub content_type: MailContentType,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReorderLessons {
    pub order: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReorderLists {
    pub order: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReorderModules {
    pub order: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReplaceContentTerms {
    pub term_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReplaceRoleGrants {
    pub grants: Vec<Grant>,
}

pub type RetryDelivery = Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Role {
    pub id: String,
    pub site_id: String,
    pub name: String,
    pub grants: Vec<Grant>,
    pub created_at: String,
    pub protected: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RoleListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RolePage {
    pub items: Vec<Role>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RotateCredential {
    pub expected_version: i64,
    pub values: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RunListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub state: Option<Value>,
}

pub type RunState = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RuntimeManifest {
    pub protocol: String,
    pub release: String,
    pub api_contract_version: String,
    pub api_contract_hash: String,
    pub storage_schema_version: i64,
    pub runtime_mode: String,
    pub site_id: String,
    pub pagination: PaginationContract,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScheduleContent {
    pub at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SeenCount {
    pub seen: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SendCampaign {
    pub template_id: String,
    pub variables: Option<Value>,
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SendCount {
    pub enqueued: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionCreated {
    pub id: String,
    pub token: String,
    pub expires_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SetupInput {
    pub site_name: String,
    pub email: String,
    pub name: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SetupStatus {
    pub initialized: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SimulateFlow {
    pub event: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Simulation {
    pub steps: Vec<SimulationStep>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SimulationStep {
    pub position: i64,
    pub kind: StepKind,
    pub config: Value,
    pub event: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SiteSettings {
    pub site_id: String,
    pub name: String,
    pub timezone: String,
    pub canonical_url: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StartDesignChange {
    pub name: String,
}

pub type StepKind = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Student {
    pub id: String,
    pub email: String,
    pub name: String,
    pub standing: StudentStanding,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StudentActivationInput {
    pub email: String,
    pub invitation_token: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StudentInvitation {
    pub student: Student,
    pub invitation_token: String,
    pub invitation_expires_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StudentListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub standing: Option<StudentStanding>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StudentLoginInput {
    pub email: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StudentPage {
    pub items: Vec<Student>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StudentSessionCreated {
    pub student: Student,
    pub token: String,
    pub expires_at: String,
}

pub type StudentStanding = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SubmissionExportFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SubmissionListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub unread: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SubmissionPage {
    pub items: Vec<FormSubmission>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SubmissionReceipt {
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SubmitForm {
    pub answers: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Term {
    pub id: String,
    pub site_id: String,
    pub kind: TermKind,
    pub language: String,
    pub slug: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub type TermKind = String;

pub type TermList = Vec<Term>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TermListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub kind: Option<TermKind>,
    pub language: Option<String>,
    pub parent_id: Option<String>,
    pub roots: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TermPage {
    pub items: Vec<Term>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TrashItem {
    pub kind: TrashKind,
    pub id: String,
    pub label: String,
    pub deleted_at: String,
}

pub type TrashKind = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TrashListFilter {
    pub after: Option<String>,
    pub limit: Option<i64>,
    pub kind: Option<TrashKind>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TrashPage {
    pub items: Vec<TrashItem>,
    pub next_cursor: Option<String>,
}

pub type Trigger = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TriggerDescription {
    pub trigger: Trigger,
    pub emitted_by: String,
}

pub type TriggerList = Vec<TriggerDescription>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UnsubscribeReceipt {
    pub unsubscribed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateBoard {
    pub name: Option<String>,
    pub description: Option<String>,
    pub archived: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateCard {
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateComment {
    pub body: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateContent {
    pub slug: Option<String>,
    pub title: Option<String>,
    pub excerpt: Option<String>,
    pub body: Option<String>,
    pub fields: Option<Value>,
    pub publication: Option<PublicationInput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateCourse {
    pub title: Option<String>,
    pub about: Option<String>,
    pub state: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateFlow {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub trigger: Option<Value>,
    pub steps: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateForm {
    pub name: Option<String>,
    pub fields: Option<Value>,
    pub open: Option<bool>,
    pub kept_days: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateLanguage {
    pub name: Option<String>,
    pub is_default: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateLesson {
    pub title: Option<String>,
    pub body: Option<String>,
    pub media_file_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateMailList {
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateMailTemplate {
    pub subject: Option<String>,
    pub body: Option<String>,
    pub content_type: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateModule {
    pub title: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdatePersonStatus {
    pub status: PersonListFilterStatus,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateProduct {
    pub name: Option<String>,
    pub description: Option<String>,
    pub price_minor: Option<i64>,
    pub stock: Option<i64>,
    pub on_sale: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateSiteSettings {
    pub name: Option<String>,
    pub timezone: Option<String>,
    pub canonical_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateStudent {
    pub name: Option<String>,
    pub standing: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateTerm {
    pub name: Option<String>,
    pub parent_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UploadFileQuery {
    pub name: String,
    pub visibility: Option<FileVisibility>,
}

pub type VariantPreset = String;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationDefinition {
    pub name: &'static str,
    pub method: &'static str,
    pub path: &'static str,
    pub request: Option<&'static str>,
    pub request_location: Option<&'static str>,
    pub query: Option<&'static str>,
    pub response: Option<&'static str>,
    pub response_location: Option<&'static str>,
    pub status: u16,
    pub authentication: &'static str,
    pub capability: Option<&'static str>,
    pub action: Option<&'static str>,
}

pub const OPERATIONS: &[OperationDefinition] = &[
    OperationDefinition { name: "setup.status", method: "get", path: "/api/v1/setup", request: None, request_location: None, query: None, response: Some("SetupStatus"), response_location: None, status: 200, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "setup.initialize", method: "post", path: "/api/v1/setup", request: Some("SetupInput"), request_location: Some("json"), query: None, response: Some("Person"), response_location: None, status: 201, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "auth.session.create", method: "post", path: "/api/v1/auth/sessions", request: Some("LoginInput"), request_location: Some("json"), query: None, response: Some("SessionCreated"), response_location: None, status: 201, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "auth.password_reset.request", method: "post", path: "/api/v1/auth/password-resets", request: Some("PasswordResetRequest"), request_location: Some("json"), query: None, response: Some("PasswordResetRequested"), response_location: None, status: 202, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "auth.password_reset.redeem", method: "post", path: "/api/v1/auth/password-resets/redeem", request: Some("PasswordResetRedeem"), request_location: Some("json"), query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "auth.email_verification.request", method: "post", path: "/api/v1/auth/email-verifications", request: Some("EmailVerificationRequest"), request_location: Some("json"), query: None, response: Some("EmailVerificationRequested"), response_location: None, status: 202, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "auth.email_verification.redeem", method: "post", path: "/api/v1/auth/email-verifications/redeem", request: Some("EmailVerificationRedeem"), request_location: Some("json"), query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "auth.session.revoke", method: "delete", path: "/api/v1/auth/sessions/current", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account", capability: None, action: None },
    OperationDefinition { name: "auth.api_key.list", method: "get", path: "/api/v1/auth/api-keys", request: Some("ApiKeyListFilter"), request_location: Some("query"), query: None, response: Some("ApiKeyPage"), response_location: None, status: 200, authentication: "account", capability: Some("people"), action: Some("view") },
    OperationDefinition { name: "auth.api_key.create", method: "post", path: "/api/v1/auth/api-keys", request: Some("CreateApiKey"), request_location: Some("json"), query: None, response: Some("ApiKeyCreated"), response_location: None, status: 201, authentication: "account", capability: Some("people"), action: Some("write") },
    OperationDefinition { name: "auth.api_key.revoke", method: "delete", path: "/api/v1/auth/api-keys/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("people"), action: Some("delete") },
    OperationDefinition { name: "people.list", method: "get", path: "/api/v1/people", request: Some("PeopleListFilter"), request_location: Some("query"), query: None, response: Some("PersonPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("people"), action: Some("view") },
    OperationDefinition { name: "people.create", method: "post", path: "/api/v1/people", request: Some("CreatePerson"), request_location: Some("json"), query: None, response: Some("PersonRecord"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("people"), action: Some("write") },
    OperationDefinition { name: "people.status.update", method: "patch", path: "/api/v1/people/{id}/status", request: Some("UpdatePersonStatus"), request_location: Some("json"), query: None, response: Some("PersonRecord"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("people"), action: Some("write") },
    OperationDefinition { name: "roles.list", method: "get", path: "/api/v1/roles", request: Some("RoleListFilter"), request_location: Some("query"), query: None, response: Some("RolePage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("people"), action: Some("view") },
    OperationDefinition { name: "roles.create", method: "post", path: "/api/v1/roles", request: Some("CreateRole"), request_location: Some("json"), query: None, response: Some("Role"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("people"), action: Some("write") },
    OperationDefinition { name: "roles.delete", method: "delete", path: "/api/v1/roles/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("people"), action: Some("delete") },
    OperationDefinition { name: "roles.grants.replace", method: "put", path: "/api/v1/roles/{id}/grants", request: Some("ReplaceRoleGrants"), request_location: Some("json"), query: None, response: Some("Role"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("people"), action: Some("write") },
    OperationDefinition { name: "content.list", method: "get", path: "/api/v1/content", request: Some("ContentListFilter"), request_location: Some("query"), query: None, response: Some("ContentPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("content"), action: Some("view") },
    OperationDefinition { name: "content.read", method: "get", path: "/api/v1/content/{id}", request: None, request_location: None, query: None, response: Some("Content"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("content"), action: Some("view") },
    OperationDefinition { name: "content.create", method: "post", path: "/api/v1/content", request: Some("CreateContent"), request_location: Some("json"), query: None, response: Some("Content"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("content"), action: Some("write") },
    OperationDefinition { name: "content.update", method: "patch", path: "/api/v1/content/{id}", request: Some("UpdateContent"), request_location: Some("json"), query: None, response: Some("Content"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("content"), action: Some("write") },
    OperationDefinition { name: "content.publish", method: "post", path: "/api/v1/content/{id}/publish", request: None, request_location: None, query: None, response: Some("Content"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("publish"), action: Some("write") },
    OperationDefinition { name: "content.schedule", method: "post", path: "/api/v1/content/{id}/schedule", request: Some("ScheduleContent"), request_location: Some("json"), query: None, response: Some("Content"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("publish"), action: Some("write") },
    OperationDefinition { name: "content.archive", method: "post", path: "/api/v1/content/{id}/archive", request: None, request_location: None, query: None, response: Some("Content"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("publish"), action: Some("write") },
    OperationDefinition { name: "content.trash", method: "delete", path: "/api/v1/content/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("trash"), action: Some("delete") },
    OperationDefinition { name: "content.restore", method: "post", path: "/api/v1/content/{id}/restore", request: None, request_location: None, query: None, response: Some("Content"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("trash"), action: Some("write") },
    OperationDefinition { name: "content.public_read", method: "get", path: "/public/v1/content/{slug}", request: Some("PublicContentQuery"), request_location: Some("query"), query: None, response: Some("Content"), response_location: None, status: 200, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "content_types.list", method: "get", path: "/api/v1/content-types", request: Some("ContentTypeListFilter"), request_location: Some("query"), query: None, response: Some("ContentTypePage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("content"), action: Some("view") },
    OperationDefinition { name: "content_types.upsert", method: "put", path: "/api/v1/content-types/{kind}", request: Some("DeclareContentType"), request_location: Some("json"), query: None, response: Some("ContentType"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("content"), action: Some("write") },
    OperationDefinition { name: "content_types.delete", method: "delete", path: "/api/v1/content-types/{kind}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("content"), action: Some("delete") },
    OperationDefinition { name: "content.revisions.list", method: "get", path: "/api/v1/content/{id}/revisions", request: Some("ContentRevisionListFilter"), request_location: Some("query"), query: None, response: Some("ContentRevisionPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("content"), action: Some("view") },
    OperationDefinition { name: "content.revisions.read", method: "get", path: "/api/v1/content/{id}/revisions/{revision}", request: None, request_location: None, query: None, response: Some("ContentRevision"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("content"), action: Some("view") },
    OperationDefinition { name: "content.revisions.restore", method: "post", path: "/api/v1/content/{id}/revisions/{revision}/restore", request: None, request_location: None, query: None, response: Some("Content"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("content"), action: Some("write") },
    OperationDefinition { name: "settings.read", method: "get", path: "/api/v1/settings", request: None, request_location: None, query: None, response: Some("SiteSettings"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("settings"), action: Some("view") },
    OperationDefinition { name: "settings.update", method: "patch", path: "/api/v1/settings", request: Some("UpdateSiteSettings"), request_location: Some("json"), query: None, response: Some("SiteSettings"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("settings"), action: Some("write") },
    OperationDefinition { name: "languages.list", method: "get", path: "/api/v1/languages", request: Some("LanguageListFilter"), request_location: Some("query"), query: None, response: Some("LanguagePage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("settings"), action: Some("view") },
    OperationDefinition { name: "languages.create", method: "post", path: "/api/v1/languages", request: Some("CreateLanguage"), request_location: Some("json"), query: None, response: Some("Language"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("settings"), action: Some("write") },
    OperationDefinition { name: "languages.update", method: "patch", path: "/api/v1/languages/{tag}", request: Some("UpdateLanguage"), request_location: Some("json"), query: None, response: Some("Language"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("settings"), action: Some("write") },
    OperationDefinition { name: "languages.delete", method: "delete", path: "/api/v1/languages/{tag}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("settings"), action: Some("delete") },
    OperationDefinition { name: "taxonomy.terms.list", method: "get", path: "/api/v1/terms", request: Some("TermListFilter"), request_location: Some("query"), query: None, response: Some("TermPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("taxonomy"), action: Some("view") },
    OperationDefinition { name: "taxonomy.terms.create", method: "post", path: "/api/v1/terms", request: Some("CreateTerm"), request_location: Some("json"), query: None, response: Some("Term"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("taxonomy"), action: Some("write") },
    OperationDefinition { name: "taxonomy.terms.read", method: "get", path: "/api/v1/terms/{id}", request: None, request_location: None, query: None, response: Some("Term"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("taxonomy"), action: Some("view") },
    OperationDefinition { name: "taxonomy.terms.update", method: "patch", path: "/api/v1/terms/{id}", request: Some("UpdateTerm"), request_location: Some("json"), query: None, response: Some("Term"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("taxonomy"), action: Some("write") },
    OperationDefinition { name: "taxonomy.terms.trash", method: "delete", path: "/api/v1/terms/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("taxonomy"), action: Some("delete") },
    OperationDefinition { name: "taxonomy.public_archive", method: "get", path: "/public/v1/terms/{kind}/{slug}", request: Some("PublicTermArchiveQuery"), request_location: Some("query"), query: None, response: Some("ContentPage"), response_location: None, status: 200, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "taxonomy.content_terms.list", method: "get", path: "/api/v1/content/{id}/terms", request: None, request_location: None, query: None, response: Some("TermList"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("taxonomy"), action: Some("view") },
    OperationDefinition { name: "taxonomy.content_terms.replace", method: "put", path: "/api/v1/content/{id}/terms", request: Some("ReplaceContentTerms"), request_location: Some("json"), query: None, response: Some("TermList"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("taxonomy"), action: Some("write") },
    OperationDefinition { name: "taxonomy.term_content.list", method: "get", path: "/api/v1/terms/{id}/content", request: Some("ContentTermAssignmentListFilter"), request_location: Some("query"), query: None, response: Some("ContentTermAssignmentPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("taxonomy"), action: Some("view") },
    OperationDefinition { name: "media.files.list", method: "get", path: "/api/v1/files", request: Some("FileListFilter"), request_location: Some("query"), query: None, response: Some("FilePage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("media"), action: Some("view") },
    OperationDefinition { name: "media.files.upload", method: "post", path: "/api/v1/files", request: Some("FileBytes"), request_location: Some("raw"), query: Some("UploadFileQuery"), response: Some("File"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("media"), action: Some("write") },
    OperationDefinition { name: "media.files.read", method: "get", path: "/api/v1/files/{id}", request: None, request_location: None, query: None, response: Some("File"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("media"), action: Some("view") },
    OperationDefinition { name: "media.files.download", method: "get", path: "/api/v1/files/{id}/content", request: None, request_location: None, query: None, response: Some("FileBytes"), response_location: Some("raw"), status: 200, authentication: "account_or_assistant", capability: Some("media"), action: Some("view") },
    OperationDefinition { name: "media.files.variants.list", method: "get", path: "/api/v1/files/{id}/variants", request: Some("FileVariantListFilter"), request_location: Some("query"), query: None, response: Some("FileVariantPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("media"), action: Some("view") },
    OperationDefinition { name: "media.files.variants.download", method: "get", path: "/api/v1/files/{id}/variants/{preset}/content", request: None, request_location: None, query: None, response: Some("FileBytes"), response_location: Some("raw"), status: 200, authentication: "account_or_assistant", capability: Some("media"), action: Some("view") },
    OperationDefinition { name: "media.files.public_download", method: "get", path: "/public/v1/files/{id}", request: None, request_location: None, query: None, response: Some("FileBytes"), response_location: Some("raw"), status: 200, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "media.files.variants.public_download", method: "get", path: "/public/v1/files/{id}/variants/{preset}", request: None, request_location: None, query: None, response: Some("FileBytes"), response_location: Some("raw"), status: 200, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "media.files.trash", method: "delete", path: "/api/v1/files/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("media"), action: Some("delete") },
    OperationDefinition { name: "audit.events.list", method: "get", path: "/api/v1/audit", request: Some("AuditListFilter"), request_location: Some("query"), query: None, response: Some("AuditEventPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("audit"), action: Some("view") },
    OperationDefinition { name: "audit.events.read", method: "get", path: "/api/v1/audit/{id}", request: None, request_location: None, query: None, response: Some("AuditEvent"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("audit"), action: Some("view") },
    OperationDefinition { name: "trash.items.list", method: "get", path: "/api/v1/trash", request: Some("TrashListFilter"), request_location: Some("query"), query: None, response: Some("TrashPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("trash"), action: Some("view") },
    OperationDefinition { name: "trash.items.restore", method: "post", path: "/api/v1/trash/{kind}/{id}/restore", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("trash"), action: Some("write") },
    OperationDefinition { name: "trash.items.delete_permanently", method: "delete", path: "/api/v1/trash/{kind}/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("trash"), action: Some("delete") },
    OperationDefinition { name: "design.changes.list", method: "get", path: "/api/v1/design/changes", request: Some("DesignChangeListFilter"), request_location: Some("query"), query: None, response: Some("DesignChangePage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("design"), action: Some("view") },
    OperationDefinition { name: "design.changes.start", method: "post", path: "/api/v1/design/changes", request: Some("StartDesignChange"), request_location: Some("json"), query: None, response: Some("DesignChange"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("design"), action: Some("write") },
    OperationDefinition { name: "design.changes.read", method: "get", path: "/api/v1/design/changes/{id}", request: None, request_location: None, query: None, response: Some("DesignChange"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("design"), action: Some("view") },
    OperationDefinition { name: "design.files.list", method: "get", path: "/api/v1/design/changes/{id}/files", request: Some("DesignFileListFilter"), request_location: Some("query"), query: None, response: Some("DesignFilePage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("design"), action: Some("view") },
    OperationDefinition { name: "design.files.read", method: "get", path: "/api/v1/design/changes/{id}/file", request: None, request_location: None, query: Some("DesignFileQuery"), response: Some("DesignFile"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("design"), action: Some("view") },
    OperationDefinition { name: "design.files.write", method: "put", path: "/api/v1/design/changes/{id}/file", request: Some("DesignFileInput"), request_location: Some("json"), query: None, response: Some("DesignFile"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("design"), action: Some("write") },
    OperationDefinition { name: "design.files.remove", method: "delete", path: "/api/v1/design/changes/{id}/file", request: None, request_location: None, query: Some("DesignFileQuery"), response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("design"), action: Some("delete") },
    OperationDefinition { name: "design.builds.create", method: "post", path: "/api/v1/design/changes/{id}/builds", request: None, request_location: None, query: None, response: Some("DesignBuild"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("design"), action: Some("write") },
    OperationDefinition { name: "design.builds.list", method: "get", path: "/api/v1/design/changes/{id}/builds", request: Some("DesignBuildListFilter"), request_location: Some("query"), query: None, response: Some("DesignBuildPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("design"), action: Some("view") },
    OperationDefinition { name: "design.changes.publish", method: "post", path: "/api/v1/design/changes/{id}/publish", request: None, request_location: None, query: None, response: Some("DesignChange"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("publish"), action: Some("write") },
    OperationDefinition { name: "design.changes.rollback", method: "post", path: "/api/v1/design/changes/{id}/rollback", request: None, request_location: None, query: None, response: Some("DesignChange"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("publish"), action: Some("write") },
    OperationDefinition { name: "design.preview.asset", method: "get", path: "/preview/v1/design/{build_id}/{path}", request: None, request_location: None, query: None, response: Some("DesignAsset"), response_location: None, status: 200, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "design.public.asset", method: "get", path: "/public/v1/site/{path}", request: None, request_location: None, query: None, response: Some("DesignAsset"), response_location: None, status: 200, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "forms.list", method: "get", path: "/api/v1/forms", request: Some("FormListFilter"), request_location: Some("query"), query: None, response: Some("FormPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("forms"), action: Some("view") },
    OperationDefinition { name: "forms.create", method: "post", path: "/api/v1/forms", request: Some("CreateForm"), request_location: Some("json"), query: None, response: Some("Form"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("forms"), action: Some("write") },
    OperationDefinition { name: "forms.read", method: "get", path: "/api/v1/forms/{id}", request: None, request_location: None, query: None, response: Some("Form"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("forms"), action: Some("view") },
    OperationDefinition { name: "forms.update", method: "patch", path: "/api/v1/forms/{id}", request: Some("UpdateForm"), request_location: Some("json"), query: None, response: Some("Form"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("forms"), action: Some("write") },
    OperationDefinition { name: "forms.delete", method: "delete", path: "/api/v1/forms/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("forms"), action: Some("delete") },
    OperationDefinition { name: "forms.submissions.list", method: "get", path: "/api/v1/forms/{id}/submissions", request: Some("SubmissionListFilter"), request_location: Some("query"), query: None, response: Some("SubmissionPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("forms"), action: Some("view") },
    OperationDefinition { name: "forms.submissions.export", method: "get", path: "/api/v1/forms/{id}/submissions/export", request: Some("SubmissionExportFilter"), request_location: Some("query"), query: None, response: Some("FormSubmissionExport"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("forms"), action: Some("view") },
    OperationDefinition { name: "forms.submissions.mark_read", method: "post", path: "/api/v1/forms/{id}/submissions/mark-read", request: None, request_location: None, query: None, response: Some("SeenCount"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("forms"), action: Some("write") },
    OperationDefinition { name: "forms.submissions.delete", method: "delete", path: "/api/v1/form-submissions/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("forms"), action: Some("delete") },
    OperationDefinition { name: "forms.public.read", method: "get", path: "/public/v1/forms/{slug}", request: None, request_location: None, query: None, response: Some("PublicForm"), response_location: None, status: 200, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "forms.public.submit", method: "post", path: "/public/v1/forms/{slug}/submissions", request: Some("SubmitForm"), request_location: Some("json"), query: None, response: Some("SubmissionReceipt"), response_location: None, status: 201, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "mail.templates.list", method: "get", path: "/api/v1/mail/templates", request: Some("MailTemplateListFilter"), request_location: Some("query"), query: None, response: Some("MailTemplatePage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("mail"), action: Some("view") },
    OperationDefinition { name: "mail.templates.create", method: "post", path: "/api/v1/mail/templates", request: Some("CreateMailTemplate"), request_location: Some("json"), query: None, response: Some("MailTemplate"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("mail"), action: Some("write") },
    OperationDefinition { name: "mail.templates.read", method: "get", path: "/api/v1/mail/templates/{id}", request: None, request_location: None, query: None, response: Some("MailTemplate"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("mail"), action: Some("view") },
    OperationDefinition { name: "mail.templates.update", method: "patch", path: "/api/v1/mail/templates/{id}", request: Some("UpdateMailTemplate"), request_location: Some("json"), query: None, response: Some("MailTemplate"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("mail"), action: Some("write") },
    OperationDefinition { name: "mail.templates.delete", method: "delete", path: "/api/v1/mail/templates/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("mail"), action: Some("delete") },
    OperationDefinition { name: "mail.templates.preview", method: "post", path: "/api/v1/mail/templates/{id}/preview", request: Some("MailTemplatePreview"), request_location: Some("json"), query: None, response: Some("RenderedMail"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("mail"), action: Some("view") },
    OperationDefinition { name: "mail.lists.list", method: "get", path: "/api/v1/mail/lists", request: Some("MailListListFilter"), request_location: Some("query"), query: None, response: Some("MailListPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("mail"), action: Some("view") },
    OperationDefinition { name: "mail.lists.create", method: "post", path: "/api/v1/mail/lists", request: Some("CreateMailList"), request_location: Some("json"), query: None, response: Some("MailList"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("mail"), action: Some("write") },
    OperationDefinition { name: "mail.lists.read", method: "get", path: "/api/v1/mail/lists/{id}", request: None, request_location: None, query: None, response: Some("MailList"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("mail"), action: Some("view") },
    OperationDefinition { name: "mail.lists.update", method: "patch", path: "/api/v1/mail/lists/{id}", request: Some("UpdateMailList"), request_location: Some("json"), query: None, response: Some("MailList"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("mail"), action: Some("write") },
    OperationDefinition { name: "mail.lists.delete", method: "delete", path: "/api/v1/mail/lists/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("mail"), action: Some("delete") },
    OperationDefinition { name: "mail.readers.list", method: "get", path: "/api/v1/mail/lists/{id}/readers", request: Some("ReaderListFilter"), request_location: Some("query"), query: None, response: Some("MailReaderPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("mail"), action: Some("view") },
    OperationDefinition { name: "mail.readers.add", method: "post", path: "/api/v1/mail/lists/{id}/readers", request: Some("AddReader"), request_location: Some("json"), query: None, response: Some("MailReaderCreated"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("mail"), action: Some("write") },
    OperationDefinition { name: "mail.readers.delete", method: "delete", path: "/api/v1/mail/readers/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("mail"), action: Some("delete") },
    OperationDefinition { name: "mail.public.unsubscribe", method: "post", path: "/public/v1/mail/unsubscribe/{token}", request: None, request_location: None, query: None, response: Some("UnsubscribeReceipt"), response_location: None, status: 200, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "mail.deliveries.list", method: "get", path: "/api/v1/mail/deliveries", request: Some("DeliveryListFilter"), request_location: Some("query"), query: None, response: Some("MailDeliveryPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("mail"), action: Some("view") },
    OperationDefinition { name: "mail.deliveries.enqueue", method: "post", path: "/api/v1/mail/deliveries", request: Some("EnqueueDelivery"), request_location: Some("json"), query: None, response: Some("MailDelivery"), response_location: None, status: 202, authentication: "account_or_assistant", capability: Some("mail"), action: Some("write") },
    OperationDefinition { name: "mail.deliveries.read", method: "get", path: "/api/v1/mail/deliveries/{id}", request: None, request_location: None, query: None, response: Some("MailDelivery"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("mail"), action: Some("view") },
    OperationDefinition { name: "mail.deliveries.retry", method: "post", path: "/api/v1/mail/deliveries/{id}/retry", request: Some("RetryDelivery"), request_location: Some("json"), query: None, response: Some("MailDelivery"), response_location: None, status: 202, authentication: "account_or_assistant", capability: Some("mail"), action: Some("write") },
    OperationDefinition { name: "mail.deliveries.campaign", method: "post", path: "/api/v1/mail/lists/{id}/deliveries", request: Some("SendCampaign"), request_location: Some("json"), query: None, response: Some("SendCount"), response_location: None, status: 202, authentication: "account_or_assistant", capability: Some("mail"), action: Some("write") },
    OperationDefinition { name: "shop.products.list", method: "get", path: "/api/v1/shop/products", request: Some("ProductListFilter"), request_location: Some("query"), query: None, response: Some("ProductPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("shop"), action: Some("view") },
    OperationDefinition { name: "shop.products.create", method: "post", path: "/api/v1/shop/products", request: Some("CreateProduct"), request_location: Some("json"), query: None, response: Some("Product"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("shop"), action: Some("write") },
    OperationDefinition { name: "shop.products.read", method: "get", path: "/api/v1/shop/products/{id}", request: None, request_location: None, query: None, response: Some("Product"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("shop"), action: Some("view") },
    OperationDefinition { name: "shop.products.update", method: "patch", path: "/api/v1/shop/products/{id}", request: Some("UpdateProduct"), request_location: Some("json"), query: None, response: Some("Product"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("shop"), action: Some("write") },
    OperationDefinition { name: "shop.products.delete", method: "delete", path: "/api/v1/shop/products/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("shop"), action: Some("delete") },
    OperationDefinition { name: "shop.public.products.list", method: "get", path: "/public/v1/shop/products", request: Some("PublicProductListFilter"), request_location: Some("query"), query: None, response: Some("PublicProductPage"), response_location: None, status: 200, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "shop.coupons.list", method: "get", path: "/api/v1/shop/coupons", request: Some("CouponListFilter"), request_location: Some("query"), query: None, response: Some("CouponPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("shop"), action: Some("view") },
    OperationDefinition { name: "shop.coupons.create", method: "post", path: "/api/v1/shop/coupons", request: Some("CreateCoupon"), request_location: Some("json"), query: None, response: Some("Coupon"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("shop"), action: Some("write") },
    OperationDefinition { name: "shop.coupons.delete", method: "delete", path: "/api/v1/shop/coupons/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("shop"), action: Some("delete") },
    OperationDefinition { name: "shop.orders.list", method: "get", path: "/api/v1/shop/orders", request: Some("OrderListFilter"), request_location: Some("query"), query: None, response: Some("OrderSummaryPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("shop"), action: Some("view") },
    OperationDefinition { name: "shop.orders.read", method: "get", path: "/api/v1/shop/orders/{id}", request: None, request_location: None, query: None, response: Some("Order"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("shop"), action: Some("view") },
    OperationDefinition { name: "shop.orders.transition", method: "post", path: "/api/v1/shop/orders/{id}/transition", request: Some("OrderTransition"), request_location: Some("json"), query: None, response: Some("Order"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("shop"), action: Some("write") },
    OperationDefinition { name: "shop.public.orders.checkout", method: "post", path: "/public/v1/shop/orders", request: Some("CheckoutInput"), request_location: Some("json"), query: None, response: Some("CheckoutReceipt"), response_location: None, status: 201, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "courses.list", method: "get", path: "/api/v1/courses", request: Some("CourseListFilter"), request_location: Some("query"), query: None, response: Some("CourseSummaryPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("courses"), action: Some("view") },
    OperationDefinition { name: "courses.create", method: "post", path: "/api/v1/courses", request: Some("CreateCourse"), request_location: Some("json"), query: None, response: Some("Course"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("courses"), action: Some("write") },
    OperationDefinition { name: "courses.read", method: "get", path: "/api/v1/courses/{id}", request: None, request_location: None, query: None, response: Some("Course"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("courses"), action: Some("view") },
    OperationDefinition { name: "courses.update", method: "patch", path: "/api/v1/courses/{id}", request: Some("UpdateCourse"), request_location: Some("json"), query: None, response: Some("Course"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("courses"), action: Some("write") },
    OperationDefinition { name: "courses.modules.reorder", method: "put", path: "/api/v1/courses/{id}/modules/order", request: Some("ReorderModules"), request_location: Some("json"), query: None, response: Some("Course"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("courses"), action: Some("write") },
    OperationDefinition { name: "courses.modules.create", method: "post", path: "/api/v1/courses/{id}/modules", request: Some("CreateModule"), request_location: Some("json"), query: None, response: Some("Module"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("courses"), action: Some("write") },
    OperationDefinition { name: "courses.modules.read", method: "get", path: "/api/v1/courses/modules/{id}", request: None, request_location: None, query: None, response: Some("Module"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("courses"), action: Some("view") },
    OperationDefinition { name: "courses.modules.update", method: "patch", path: "/api/v1/courses/modules/{id}", request: Some("UpdateModule"), request_location: Some("json"), query: None, response: Some("Module"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("courses"), action: Some("write") },
    OperationDefinition { name: "courses.modules.delete", method: "delete", path: "/api/v1/courses/modules/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("courses"), action: Some("delete") },
    OperationDefinition { name: "courses.lessons.list", method: "get", path: "/api/v1/courses/modules/{id}/lessons", request: Some("LessonListFilter"), request_location: Some("query"), query: None, response: Some("LessonPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("courses"), action: Some("view") },
    OperationDefinition { name: "courses.lessons.reorder", method: "put", path: "/api/v1/courses/modules/{id}/lessons/order", request: Some("ReorderLessons"), request_location: Some("json"), query: None, response: Some("Module"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("courses"), action: Some("write") },
    OperationDefinition { name: "courses.lessons.create", method: "post", path: "/api/v1/courses/modules/{id}/lessons", request: Some("CreateLesson"), request_location: Some("json"), query: None, response: Some("Lesson"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("courses"), action: Some("write") },
    OperationDefinition { name: "courses.lessons.update", method: "patch", path: "/api/v1/courses/lessons/{id}", request: Some("UpdateLesson"), request_location: Some("json"), query: None, response: Some("Lesson"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("courses"), action: Some("write") },
    OperationDefinition { name: "courses.lessons.delete", method: "delete", path: "/api/v1/courses/lessons/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("courses"), action: Some("delete") },
    OperationDefinition { name: "courses.students.list", method: "get", path: "/api/v1/courses/students", request: Some("StudentListFilter"), request_location: Some("query"), query: None, response: Some("StudentPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("courses"), action: Some("view") },
    OperationDefinition { name: "courses.students.create", method: "post", path: "/api/v1/courses/students", request: Some("CreateStudent"), request_location: Some("json"), query: None, response: Some("StudentInvitation"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("courses"), action: Some("write") },
    OperationDefinition { name: "courses.students.invite", method: "post", path: "/api/v1/courses/students/{id}/invite", request: None, request_location: None, query: None, response: Some("StudentInvitation"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("courses"), action: Some("write") },
    OperationDefinition { name: "courses.students.update", method: "patch", path: "/api/v1/courses/students/{id}", request: Some("UpdateStudent"), request_location: Some("json"), query: None, response: Some("Student"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("courses"), action: Some("write") },
    OperationDefinition { name: "courses.enrollments.list", method: "get", path: "/api/v1/courses/{course_id}/enrollments", request: Some("EnrollmentListFilter"), request_location: Some("query"), query: None, response: Some("EnrollmentPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("courses"), action: Some("view") },
    OperationDefinition { name: "courses.enrollments.create", method: "post", path: "/api/v1/courses/{course_id}/enrollments", request: Some("EnrollStudent"), request_location: Some("json"), query: None, response: Some("Enrollment"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("courses"), action: Some("write") },
    OperationDefinition { name: "courses.enrollments.delete", method: "delete", path: "/api/v1/courses/enrollments/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("courses"), action: Some("delete") },
    OperationDefinition { name: "courses.students.activate", method: "post", path: "/public/v1/courses/students/activate", request: Some("StudentActivationInput"), request_location: Some("json"), query: None, response: Some("StudentSessionCreated"), response_location: None, status: 201, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "courses.students.session.create", method: "post", path: "/public/v1/courses/students/sessions", request: Some("StudentLoginInput"), request_location: Some("json"), query: None, response: Some("StudentSessionCreated"), response_location: None, status: 201, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "courses.students.session.revoke", method: "delete", path: "/student/v1/auth/session", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "student", capability: None, action: None },
    OperationDefinition { name: "learning.courses.list", method: "get", path: "/student/v1/learning/courses", request: Some("LearningCourseListFilter"), request_location: Some("query"), query: None, response: Some("LearningCoursePage"), response_location: None, status: 200, authentication: "student", capability: None, action: None },
    OperationDefinition { name: "learning.lesson.read", method: "get", path: "/student/v1/learning/lessons/{id}", request: None, request_location: None, query: None, response: Some("LearningLesson"), response_location: None, status: 200, authentication: "student", capability: None, action: None },
    OperationDefinition { name: "learning.lesson.media.read", method: "get", path: "/student/v1/learning/lessons/{id}/media", request: None, request_location: None, query: None, response: Some("FileBytes"), response_location: Some("raw"), status: 200, authentication: "student", capability: None, action: None },
    OperationDefinition { name: "learning.lesson.done", method: "put", path: "/student/v1/learning/lessons/{id}/done", request: None, request_location: None, query: None, response: Some("Progress"), response_location: None, status: 200, authentication: "student", capability: None, action: None },
    OperationDefinition { name: "jobs.list", method: "get", path: "/api/v1/jobs", request: Some("JobListFilter"), request_location: Some("query"), query: None, response: Some("JobPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("automation"), action: Some("view") },
    OperationDefinition { name: "jobs.read", method: "get", path: "/api/v1/jobs/{id}", request: None, request_location: None, query: None, response: Some("Job"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("automation"), action: Some("view") },
    OperationDefinition { name: "jobs.retry", method: "post", path: "/api/v1/jobs/{id}/retry", request: None, request_location: None, query: None, response: Some("Job"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("automation"), action: Some("write") },
    OperationDefinition { name: "automation.triggers.list", method: "get", path: "/api/v1/automation/triggers", request: None, request_location: None, query: None, response: Some("TriggerList"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("automation"), action: Some("view") },
    OperationDefinition { name: "automation.flows.list", method: "get", path: "/api/v1/automation/flows", request: Some("FlowListFilter"), request_location: Some("query"), query: None, response: Some("FlowPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("automation"), action: Some("view") },
    OperationDefinition { name: "automation.flows.create", method: "post", path: "/api/v1/automation/flows", request: Some("CreateFlow"), request_location: Some("json"), query: None, response: Some("Flow"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("automation"), action: Some("write") },
    OperationDefinition { name: "automation.flows.read", method: "get", path: "/api/v1/automation/flows/{id}", request: None, request_location: None, query: None, response: Some("Flow"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("automation"), action: Some("view") },
    OperationDefinition { name: "automation.flows.update", method: "patch", path: "/api/v1/automation/flows/{id}", request: Some("UpdateFlow"), request_location: Some("json"), query: None, response: Some("Flow"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("automation"), action: Some("write") },
    OperationDefinition { name: "automation.flows.delete", method: "delete", path: "/api/v1/automation/flows/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("automation"), action: Some("write") },
    OperationDefinition { name: "automation.flows.simulate", method: "post", path: "/api/v1/automation/flows/{id}/simulate", request: Some("SimulateFlow"), request_location: Some("json"), query: None, response: Some("Simulation"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("automation"), action: Some("view") },
    OperationDefinition { name: "automation.runs.list", method: "get", path: "/api/v1/automation/flows/{id}/runs", request: Some("RunListFilter"), request_location: Some("query"), query: None, response: Some("FlowRunPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("automation"), action: Some("view") },
    OperationDefinition { name: "automation.runs.read", method: "get", path: "/api/v1/automation/runs/{id}", request: None, request_location: None, query: None, response: Some("FlowRun"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("automation"), action: Some("view") },
    OperationDefinition { name: "boards.list", method: "get", path: "/api/v1/boards", request: Some("BoardListFilter"), request_location: Some("query"), query: None, response: Some("BoardPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("boards"), action: Some("view") },
    OperationDefinition { name: "boards.create", method: "post", path: "/api/v1/boards", request: Some("CreateBoard"), request_location: Some("json"), query: None, response: Some("Board"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("boards"), action: Some("write") },
    OperationDefinition { name: "boards.read", method: "get", path: "/api/v1/boards/{id}", request: None, request_location: None, query: None, response: Some("Board"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("boards"), action: Some("view") },
    OperationDefinition { name: "boards.update", method: "patch", path: "/api/v1/boards/{id}", request: Some("UpdateBoard"), request_location: Some("json"), query: None, response: Some("Board"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("boards"), action: Some("write") },
    OperationDefinition { name: "boards.delete", method: "delete", path: "/api/v1/boards/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("boards"), action: Some("delete") },
    OperationDefinition { name: "boards.lists.list", method: "get", path: "/api/v1/boards/{id}/lists", request: Some("ListPageFilter"), request_location: Some("query"), query: None, response: Some("BoardListPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("boards"), action: Some("view") },
    OperationDefinition { name: "boards.lists.create", method: "post", path: "/api/v1/boards/{id}/lists", request: Some("CreateList"), request_location: Some("json"), query: None, response: Some("BoardList"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("boards"), action: Some("write") },
    OperationDefinition { name: "boards.lists.reorder", method: "put", path: "/api/v1/boards/{id}/lists/order", request: Some("ReorderLists"), request_location: Some("json"), query: None, response: Some("BoardListPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("boards"), action: Some("write") },
    OperationDefinition { name: "boards.cards.list", method: "get", path: "/api/v1/boards/lists/{id}/cards", request: Some("CardPageFilter"), request_location: Some("query"), query: None, response: Some("CardPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("boards"), action: Some("view") },
    OperationDefinition { name: "boards.cards.create", method: "post", path: "/api/v1/boards/lists/{id}/cards", request: Some("CreateCard"), request_location: Some("json"), query: None, response: Some("Card"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("boards"), action: Some("write") },
    OperationDefinition { name: "boards.cards.read", method: "get", path: "/api/v1/boards/cards/{id}", request: None, request_location: None, query: None, response: Some("Card"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("boards"), action: Some("view") },
    OperationDefinition { name: "boards.cards.update", method: "patch", path: "/api/v1/boards/cards/{id}", request: Some("UpdateCard"), request_location: Some("json"), query: None, response: Some("Card"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("boards"), action: Some("write") },
    OperationDefinition { name: "boards.cards.move", method: "post", path: "/api/v1/boards/cards/{id}/move", request: Some("MoveCard"), request_location: Some("json"), query: None, response: Some("Card"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("boards"), action: Some("write") },
    OperationDefinition { name: "boards.cards.assign", method: "post", path: "/api/v1/boards/cards/{id}/assign", request: Some("AssignCard"), request_location: Some("json"), query: None, response: Some("Card"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("boards"), action: Some("write") },
    OperationDefinition { name: "boards.comments.list", method: "get", path: "/api/v1/boards/cards/{id}/comments", request: Some("CommentPageFilter"), request_location: Some("query"), query: None, response: Some("CommentPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("boards"), action: Some("view") },
    OperationDefinition { name: "boards.comments.create", method: "post", path: "/api/v1/boards/cards/{id}/comments", request: Some("CreateComment"), request_location: Some("json"), query: None, response: Some("Comment"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("boards"), action: Some("write") },
    OperationDefinition { name: "boards.comments.update", method: "patch", path: "/api/v1/boards/comments/{id}", request: Some("UpdateComment"), request_location: Some("json"), query: None, response: Some("Comment"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("boards"), action: Some("write") },
    OperationDefinition { name: "boards.comments.delete", method: "delete", path: "/api/v1/boards/comments/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("boards"), action: Some("delete") },
    OperationDefinition { name: "boards.activity.list", method: "get", path: "/api/v1/boards/{id}/activity", request: Some("ActivityPageFilter"), request_location: Some("query"), query: None, response: Some("ActivityPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("boards"), action: Some("view") },
    OperationDefinition { name: "analytics.events.ingest", method: "post", path: "/public/v1/analytics/events", request: Some("AnalyticsEventBatch"), request_location: Some("json"), query: None, response: Some("AnalyticsReceipt"), response_location: None, status: 202, authentication: "public", capability: None, action: None },
    OperationDefinition { name: "analytics.events.list", method: "get", path: "/api/v1/analytics/events", request: Some("EventListFilter"), request_location: Some("query"), query: None, response: Some("AnalyticsEventPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("analytics"), action: Some("view") },
    OperationDefinition { name: "analytics.daily.list", method: "get", path: "/api/v1/analytics/daily", request: Some("DailyListFilter"), request_location: Some("query"), query: None, response: Some("DailyAggregatePage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("analytics"), action: Some("view") },
    OperationDefinition { name: "analytics.retention.prune", method: "post", path: "/api/v1/analytics/prune", request: Some("PruneAnalytics"), request_location: Some("json"), query: None, response: Some("PruneReceipt"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("analytics"), action: Some("delete") },
    OperationDefinition { name: "portable.export", method: "get", path: "/api/v1/portable/export", request: None, request_location: None, query: None, response: Some("PortableBundle"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("portable"), action: Some("view") },
    OperationDefinition { name: "portable.import", method: "post", path: "/api/v1/portable/import", request: Some("PortableImportRequest"), request_location: Some("json"), query: None, response: Some("ImportReceipt"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("portable"), action: Some("write") },
    OperationDefinition { name: "credentials.list", method: "get", path: "/api/v1/credentials", request: Some("CredentialListFilter"), request_location: Some("query"), query: None, response: Some("CredentialPage"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("credentials"), action: Some("view") },
    OperationDefinition { name: "credentials.create", method: "post", path: "/api/v1/credentials", request: Some("CreateCredential"), request_location: Some("json"), query: None, response: Some("Credential"), response_location: None, status: 201, authentication: "account_or_assistant", capability: Some("credentials"), action: Some("write") },
    OperationDefinition { name: "credentials.rotate", method: "put", path: "/api/v1/credentials/{id}", request: Some("RotateCredential"), request_location: Some("json"), query: None, response: Some("Credential"), response_location: None, status: 200, authentication: "account_or_assistant", capability: Some("credentials"), action: Some("write") },
    OperationDefinition { name: "credentials.revoke", method: "delete", path: "/api/v1/credentials/{id}", request: None, request_location: None, query: None, response: Some("Empty"), response_location: None, status: 204, authentication: "account_or_assistant", capability: Some("credentials"), action: Some("delete") },
    OperationDefinition { name: "runtime.manifest.read", method: "get", path: "/api/v1/runtime/manifest", request: None, request_location: None, query: None, response: Some("RuntimeManifest"), response_location: None, status: 200, authentication: "public", capability: None, action: None },
];
