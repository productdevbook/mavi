// Generated from the canonical Mavi API. Do not edit by hand.

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };

export interface Activity {
  id: string;
  board_id: string;
  card_id: string | null;
  kind: string;
  actor_kind: string;
  actor_id: string | null;
  detail: Record<string, unknown>;
  created_at: string;
}

export interface ActivityPage {
  items: Activity[];
  next_cursor: string | null;
}

export interface ActivityPageFilter {
  after?: string | null;
  limit?: number;
  card_id?: string | null;
}

export interface AddReader {
  email: string;
  name?: string | null;
  resubscribe?: boolean;
}

export interface AnalyticsEvent {
  id: string;
  event_name: string;
  path: string;
  value: number;
  occurred_at: string;
  created_at: string;
}

export interface AnalyticsEventBatch {
  events: AnalyticsEventInput[];
}

export interface AnalyticsEventInput {
  event_name: string;
  path: string;
  value: number | null;
  occurred_at: string | null;
}

export interface AnalyticsEventPage {
  items: AnalyticsEvent[];
  next_cursor: string | null;
}

export interface AnalyticsReceipt {
  accepted: number;
}

export interface AnalyticsRetention {
  raw_days: number;
  aggregate_days: number;
}

export type AnalyticsRetentionUpdate = Record<string, unknown> | null;

export interface ApiKeyCreated {
  id: string;
  site_id: string;
  person_id: string;
  name: string;
  prefix: string;
  token: string;
  grants: Grant[];
  expires_at: string | null;
  created_at: string;
}

export interface ApiKeyListFilter {
  after?: string | null;
  limit?: number;
  revoked?: boolean | null;
}

export interface ApiKeyPage {
  items: ApiKeyRecord[];
  next_cursor: string | null;
}

export interface ApiKeyRecord {
  id: string;
  site_id: string;
  person_id: string;
  name: string;
  prefix: string;
  grants: Grant[];
  expires_at: string | null;
  revoked_at: string | null;
  created_at: string;
}

export interface AssignCard {
  assignee_id: string | null;
}

export type AuditActorKind = "public" | "account" | "student" | "assistant" | "system";

export interface AuditEvent {
  id: string;
  request_id: string;
  actor_kind: AuditActorKind;
  actor_id: string | null;
  action: string;
  resource_type: string;
  resource_id: string | null;
  payload: Record<string, unknown>;
  created_at: string;
}

export interface AuditEventPage {
  items: AuditEvent[];
  next_cursor: string | null;
}

export interface AuditListFilter {
  after?: string | null;
  limit?: number;
  action?: string | null;
  resource_type?: string | null;
  resource_id?: string | null;
  actor_kind?: AuditActorKind;
  actor_id?: string | null;
}

export interface BasketItem {
  product_id: string;
  quantity: number;
}

export interface Board {
  id: string;
  name: string;
  description: string | null;
  archived: boolean;
  created_at: string;
  updated_at: string;
}

export interface BoardList {
  id: string;
  board_id: string;
  name: string;
  position: number;
  created_at: string;
  updated_at: string;
}

export interface BoardListFilter {
  after?: string | null;
  limit?: number;
  archived?: boolean | null;
}

export interface BoardListPage {
  items: BoardList[];
  next_cursor: string | null;
}

export interface BoardPage {
  items: Board[];
  next_cursor: string | null;
}

export interface Card {
  id: string;
  board_id: string;
  list_id: string;
  title: string;
  description: string | null;
  assignee_id: string | null;
  position: number;
  created_at: string;
  updated_at: string;
}

export interface CardPage {
  items: Card[];
  next_cursor: string | null;
}

export interface CardPageFilter {
  after?: string | null;
  limit?: number;
  assignee_id?: string | null;
}

export interface CheckoutInput {
  email: string;
  items: BasketItem[];
  coupon_code?: string | null;
  idempotency_key: string;
}

export interface CheckoutReceipt {
  id: string;
  number: number;
  state: OrderState;
  total: Money;
}

export interface Comment {
  id: string;
  board_id: string;
  card_id: string;
  author_id: string | null;
  body: string;
  edited_at: string | null;
  created_at: string;
}

export interface CommentPage {
  items: Comment[];
  next_cursor: string | null;
}

export interface CommentPageFilter {
  after?: string | null;
  limit?: number;
}

export interface Content {
  id: string;
  site_id: string;
  kind: string;
  language: string;
  slug: string;
  title: string;
  excerpt: string | null;
  body: string;
  fields: Record<string, unknown>;
  publication: Publication;
  revision: number;
  created_at: string;
  updated_at: string;
}

export type ContentFieldKind = "text" | "long" | "email" | "number" | "choice" | "boolean";

export interface ContentListFilter {
  after?: string | null;
  limit?: number;
  kind?: string | null;
  language?: string | null;
  status?: PublicationStatus;
}

export interface ContentPage {
  items: Content[];
  next_cursor: string | null;
}

export interface ContentRevision {
  content_id: string;
  revision: number;
  kind: string;
  language: string;
  slug: string;
  title: string;
  excerpt: string | null;
  body: string;
  fields: Record<string, unknown>;
  publication: Publication;
  created_at: string;
}

export interface ContentRevisionListFilter {
  after?: string | null;
  limit?: number;
}

export interface ContentRevisionPage {
  items: ContentRevision[];
  next_cursor: string | null;
}

export interface ContentTermAssignment {
  content_id: string;
  assigned_at: string;
}

export interface ContentTermAssignmentListFilter {
  after?: string | null;
  limit?: number;
}

export interface ContentTermAssignmentPage {
  items: ContentTermAssignment[];
  next_cursor: string | null;
}

export interface ContentType {
  site_id: string;
  kind: string;
  name: string;
  fields: ContentTypeField[];
  created_at: string;
  updated_at: string;
}

export interface ContentTypeField {
  key: string;
  label: string;
  required: boolean;
  kind: ContentFieldKind;
  options: string[];
}

export interface ContentTypeListFilter {
  after?: string | null;
  limit?: number;
}

export interface ContentTypePage {
  items: ContentType[];
  next_cursor: string | null;
}

export interface Coupon {
  id: string;
  code: string;
  kind: CouponKind;
  percent: number | null;
  amount: Money | unknown;
  max_uses: number | null;
  expires_at: string | null;
  created_at: string;
  updated_at: string;
}

export type CouponKind = "percent" | "amount";

export interface CouponListFilter {
  after?: string | null;
  limit?: number;
}

export interface CouponPage {
  items: Coupon[];
  next_cursor: string | null;
}

export interface Course {
  id: string;
  slug: string;
  title: string;
  about: string | null;
  state: CourseState;
  modules: Module[];
  created_at: string;
  updated_at: string;
}

export interface CourseInstructor {
  course_id: string;
  person_id: string;
  grants: CourseInstructorGrant[];
  created_at: string;
  updated_at: string;
}

export type CourseInstructorGrant = "view" | "write" | "delete";

export interface CourseInstructorListFilter {
  after?: string | null;
  limit?: number;
}

export interface CourseInstructorPage {
  items: CourseInstructor[];
  next_cursor: string | null;
}

export interface CourseListFilter {
  after?: string | null;
  limit?: number;
  state?: CourseState;
}

export type CourseState = "draft" | "open" | "closed";

export interface CourseSummary {
  id: string;
  slug: string;
  title: string;
  about: string | null;
  state: CourseState;
  created_at: string;
  updated_at: string;
}

export interface CourseSummaryPage {
  items: CourseSummary[];
  next_cursor: string | null;
}

export interface CreateApiKey {
  name: string;
  grants: Grant[];
  expires_at?: string | null;
}

export interface CreateBoard {
  name: string;
  description?: string | null;
}

export interface CreateCard {
  title: string;
  description: string | null;
  assignee_id: string | null;
}

export interface CreateComment {
  body: string;
}

export interface CreateContent {
  kind: string;
  language: string;
  slug: string;
  title: string;
  excerpt?: string | null;
  body?: string;
  fields?: Record<string, unknown>;
  publication?: PublicationInput;
}

export interface CreateCoupon {
  code: string;
  percent?: number | null;
  amount_minor?: number | null;
  currency?: string | null;
  max_uses?: number | null;
  expires_at?: string | null;
}

export interface CreateCourse {
  slug: string;
  title: string;
  about?: string | null;
}

export interface CreateCredential {
  provider: string;
  name: string;
  values: Record<string, unknown>;
}

export interface CreateFlow {
  name: string;
  trigger: Trigger;
  steps: FlowStepInput[];
}

export interface CreateForm {
  slug: string;
  name: string;
  fields?: FormField[];
  kept_days?: number | null;
}

export interface CreateLanguage {
  tag: string;
  name: string;
  is_default?: boolean;
}

export interface CreateLesson {
  title: string;
  body?: string;
  media_file_id?: string | null;
}

export interface CreateList {
  name: string;
}

export interface CreateMailList {
  slug: string;
  name: string;
}

export interface CreateMailTemplate {
  key: string;
  language: string;
  subject: string;
  body: string;
  content_type?: MailContentType;
}

export interface CreateModule {
  title: string;
}

export interface CreatePerson {
  email: string;
  name: string;
  password: string;
  role_ids?: string[];
}

export interface CreateProduct {
  slug: string;
  name: string;
  description?: string | null;
  price: ProductPrice;
  stock: number;
  on_sale?: boolean;
}

export interface CreateReport {
  kind: ReportKind;
  title: string;
  body?: string;
  context?: Record<string, unknown>;
}

export interface CreateRole {
  name: string;
  grants?: Grant[];
}

export interface CreateStudent {
  email: string;
  name: string;
}

export interface CreateTerm {
  kind: TermKind;
  language: string;
  slug: string;
  name: string;
  parent_id?: string | null;
}

export interface Credential {
  id: string;
  site_id: string;
  provider: string;
  name: string;
  state: "active" | "revoked";
  version: number;
  created_at: string;
  updated_at: string;
}

export interface CredentialListFilter {
  after?: string | null;
  limit?: number;
}

export interface CredentialPage {
  items: Credential[];
  next_cursor: string | null;
}

export interface CurrentSession {
  person: PersonRecord;
  grants: Grant[];
}

export interface DailyAggregate {
  day: string;
  event_name: string;
  path: string;
  event_count: number;
  value_sum: number;
  value_min: number;
  value_max: number;
}

export interface DailyAggregatePage {
  items: DailyAggregate[];
  next_cursor: string | null;
}

export interface DailyListFilter {
  after?: string | null;
  limit?: number;
  event_name?: string | null;
  path?: string | null;
}

export interface DeclareContentType {
  name: string;
  fields?: ContentTypeField[];
}

export interface DeliveryListFilter {
  after?: string | null;
  limit?: number;
  status?: MailDeliveryStatus;
}

export type DesignAsset = Blob;

export interface DesignBuild {
  id: string;
  change_id: string;
  state: DesignBuildState;
  error: string | null;
  preview_path: string;
  created_at: string;
  completed_at: string | null;
}

export interface DesignBuildListFilter {
  after?: string | null;
  limit?: number;
}

export interface DesignBuildPage {
  items: DesignBuild[];
  next_cursor: string | null;
}

export type DesignBuildState = "queued" | "ready" | "failed";

export interface DesignChange {
  id: string;
  name: string;
  state: DesignState;
  ready_build_id: string | null;
  published_build_id: string | null;
  last_error: string | null;
  published_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface DesignChangeListFilter {
  after?: string | null;
  limit?: number;
  state?: DesignState;
}

export interface DesignChangePage {
  items: DesignChange[];
  next_cursor: string | null;
}

export interface DesignFile {
  path: string;
  contents: string;
  bytes: number;
  sha256: string;
  removed: boolean;
  updated_at: string;
}

export interface DesignFileInput {
  path: string;
  contents: string;
}

export interface DesignFileListFilter {
  after?: string | null;
  limit?: number;
}

export interface DesignFilePage {
  items: DesignFileSummary[];
  next_cursor: string | null;
}

export interface DesignFileQuery {
  path: string;
}

export interface DesignFileSummary {
  path: string;
  bytes: number;
  sha256: string;
  removed: boolean;
  updated_at: string;
}

export type DesignState = "writing" | "building" | "ready" | "failed" | "published";

export interface EmailVerificationRedeem {
  token: string;
}

export interface EmailVerificationRequest {
  email: string;
}

export interface EmailVerificationRequested {
  accepted: boolean;
}

export type Empty = Record<string, unknown>;

export interface EnqueueDelivery {
  recipient: string;
  template_id: string;
  variables?: Record<string, unknown>;
  idempotency_key?: string | null;
}

export interface EnrollStudent {
  student_id: string;
}

export interface Enrollment {
  id: string;
  course_id: string;
  student_id: string;
  started_at: string;
  finished_at: string | null;
  created_at: string;
}

export interface EnrollmentListFilter {
  after?: string | null;
  limit?: number;
}

export interface EnrollmentPage {
  items: Enrollment[];
  next_cursor: string | null;
}

export interface ErrorBody {
  code: string;
  message: string;
  field?: string | null;
}

export interface ErrorEnvelope {
  error: ErrorBody;
}

export interface EventListFilter {
  after?: string | null;
  limit?: number;
  event_name?: string | null;
  path?: string | null;
}

export interface FeedbackReport {
  id: string;
  reporter_kind: "account" | "assistant";
  kind: ReportKind;
  title: string;
  body: string;
  context: Record<string, unknown>;
  state: ReportState;
  answer: string | null;
  created_at: string;
  updated_at: string;
}

export interface FeedbackReportPage {
  items: FeedbackReport[];
  next_cursor: string | null;
}

export interface File {
  id: string;
  kind: FileKind;
  visibility: FileVisibility;
  mime: string;
  name: string;
  bytes: number;
  sha256: string;
  created_at: string;
}

export type FileBytes = Blob;

export type FileKind = "image" | "video" | "audio" | "document";

export interface FileListFilter {
  after?: string | null;
  limit?: number;
  kind?: FileKind;
}

export interface FilePage {
  items: File[];
  next_cursor: string | null;
}

export interface FileVariant {
  id: string;
  source_file_id: string;
  preset: VariantPreset;
  mime: string;
  width: number;
  height: number;
  bytes: number;
  sha256: string;
  created_at: string;
}

export interface FileVariantListFilter {
  after?: string | null;
  limit?: number;
}

export interface FileVariantPage {
  items: FileVariant[];
  next_cursor: string | null;
}

export type FileVisibility = "private" | "public";

export interface Flow {
  id: string;
  name: string;
  trigger: Trigger;
  enabled: boolean;
  version: number;
  steps: FlowStep[];
  created_at: string;
  updated_at: string;
}

export interface FlowListFilter {
  after?: string | null;
  limit?: number;
  trigger?: unknown;
  enabled?: boolean | null;
}

export interface FlowPage {
  items: Flow[];
  next_cursor: string | null;
}

export interface FlowRun {
  id: string;
  flow_id: string;
  trigger: Trigger;
  event: Record<string, unknown>;
  definition: FlowStepInput[];
  state: RunState;
  current_position: number;
  retry_count: number;
  last_error: string | null;
  steps: FlowRunStep[];
  started_at: string;
  finished_at: string | null;
}

export interface FlowRunPage {
  items: FlowRun[];
  next_cursor: string | null;
}

export interface FlowRunStep {
  id: string;
  position: number;
  attempt: number;
  kind: StepKind;
  outcome: string;
  detail: Record<string, unknown>;
  error: string | null;
  started_at: string;
  finished_at: string;
}

export interface FlowStep {
  id: string;
  position: number;
  kind: StepKind;
  config: Record<string, unknown>;
}

export interface FlowStepInput {
  kind: StepKind;
  config: Record<string, unknown>;
}

export interface Form {
  id: string;
  slug: string;
  name: string;
  fields: FormField[];
  open: boolean;
  kept_days: number;
  created_at: string;
  updated_at: string;
}

export interface FormExportMetadata {
  id: string;
  slug: string;
  name: string;
  fields: FormField[];
  kept_days: number;
}

export interface FormField {
  key: string;
  label: string;
  required: boolean;
  kind: FormFieldKind;
  options: string[];
}

export type FormFieldKind = "text" | "long" | "email" | "number" | "choice" | "boolean";

export interface FormListFilter {
  after?: string | null;
  limit?: number;
}

export interface FormPage {
  items: Form[];
  next_cursor: string | null;
}

export interface FormSubmission {
  id: string;
  form_id: string;
  answers: Record<string, unknown>;
  seen_at: string | null;
  created_at: string;
}

export interface FormSubmissionExport {
  format: string;
  version: number;
  form: FormExportMetadata;
  items: FormSubmission[];
  next_cursor: string | null;
}

export interface Grant {
  capability: string;
  action: string;
}

export interface ImportReceipt {
  strategy: ImportStrategy;
  languages: number;
  content_types: number;
  terms: number;
  content: number;
  revisions: number;
  slug_history: number;
  assignments: number;
}

export type ImportStrategy = "validate_only" | "create_only" | "upsert";

export interface Job {
  id: string;
  kind: string;
  payload: Record<string, unknown>;
  state: JobState;
  run_at: string;
  claimed_until: string | null;
  claimed_by: string | null;
  attempts: number;
  last_error: string | null;
  idempotency_key: string | null;
  created_at: string;
  finished_at: string | null;
}

export interface JobListFilter {
  after?: string | null;
  limit?: number;
  state?: JobState;
  kind?: string | null;
}

export interface JobPage {
  items: Job[];
  next_cursor: string | null;
}

export type JobState = "ready" | "running" | "done" | "dead";

export interface Language {
  site_id: string;
  tag: string;
  name: string;
  is_default: boolean;
  created_at: string;
  updated_at: string;
}

export interface LanguageListFilter {
  after?: string | null;
  limit?: number;
}

export interface LanguagePage {
  items: Language[];
  next_cursor: string | null;
}

export interface LearningCourse {
  course_id: string;
  slug: string;
  title: string;
  about: string | null;
  state: CourseState;
  completed_lessons: number;
  total_lessons: number;
  enrolled_at: string;
}

export interface LearningCourseDetail {
  course: LearningCourse;
  modules: LearningModule[];
}

export interface LearningCourseListFilter {
  after?: string | null;
  limit?: number;
}

export interface LearningCoursePage {
  items: LearningCourse[];
  next_cursor: string | null;
}

export interface LearningLesson {
  lesson: Lesson;
  completed_at: string | null;
  course_id: string;
  course: string;
  position: number;
  total: number;
  previous: string | null;
  next: string | null;
}

export interface LearningLessonSummary {
  id: string;
  module_id: string;
  title: string;
  media_file_id: string | null;
  position: number;
  completed_at: string | null;
}

export interface LearningModule {
  id: string;
  course_id: string;
  title: string;
  position: number;
  lessons: LearningLessonSummary[];
}

export interface Lesson {
  id: string;
  module_id: string;
  title: string;
  body: string;
  media_file_id: string | null;
  position: number;
  created_at: string;
  updated_at: string;
}

export interface LessonListFilter {
  after?: string | null;
  limit?: number;
}

export interface LessonPage {
  items: Lesson[];
  next_cursor: string | null;
}

export interface ListPageFilter {
  after?: string | null;
  limit?: number;
}

export interface LoginInput {
  email: string;
  password: string;
}

export type MailBounceClass = "transient" | "permanent";

export type MailContentType = "plain" | "html";

export interface MailDelivery {
  id: string;
  template_id: string | null;
  list_id: string | null;
  recipient: string;
  sender: MailSender;
  subject: string;
  body: string;
  body_protected: boolean;
  content_type: MailContentType;
  purpose: MailPurpose;
  status: MailDeliveryStatus;
  attempts: number;
  available_at: string;
  provider: string | null;
  provider_reference: string | null;
  last_error: string | null;
  created_at: string;
  updated_at: string;
  sent_at: string | null;
}

export interface MailDeliveryPage {
  items: MailDelivery[];
  next_cursor: string | null;
}

export type MailDeliveryStatus = "queued" | "sending" | "retry" | "sent" | "dead" | "cancelled";

export interface MailList {
  id: string;
  slug: string;
  name: string;
  subscriber_count: number;
  created_at: string;
  updated_at: string;
}

export interface MailListListFilter {
  after?: string | null;
  limit?: number;
}

export interface MailListPage {
  items: MailList[];
  next_cursor: string | null;
}

export type MailProviderEventKind = "delivered" | "bounced" | "complained";

export interface MailProviderEventReceipt {
  duplicate: boolean;
  suppressed: boolean;
  cancelled_deliveries: number;
}

export type MailPurpose = "transactional" | "campaign";

export interface MailReader {
  id: string;
  email: string;
  name: string | null;
  standing: MailStanding;
  added_at: string;
}

export interface MailReaderCreated {
  reader: MailReader;
  unsubscribe_token: string;
}

export interface MailReaderPage {
  items: MailReader[];
  next_cursor: string | null;
}

export type MailSender = Record<string, unknown> | null;

export type MailSenderUpdate = Record<string, unknown> | null;

export type MailStanding = "subscribed" | "unsubscribed" | "bounced" | "complained";

export interface MailTemplate {
  id: string;
  key: string;
  language: string;
  subject: string;
  body: string;
  content_type: MailContentType;
  variables: string[];
  created_at: string;
  updated_at: string;
}

export interface MailTemplateListFilter {
  after?: string | null;
  limit?: number;
}

export interface MailTemplatePage {
  items: MailTemplate[];
  next_cursor: string | null;
}

export interface MailTemplatePreview {
  variables?: Record<string, unknown>;
}

export interface Module {
  id: string;
  course_id: string;
  title: string;
  position: number;
  lessons: Lesson[];
  created_at: string;
  updated_at: string;
}

export interface Money {
  minor: number;
  currency: string;
}

export interface MoveCard {
  list_id: string;
  before_card_id: string | null;
}

export interface Order {
  id: string;
  number: number;
  state: OrderState;
  email: string;
  total: Money;
  lines: OrderLine[];
  payment_provider: string | null;
  payment_reference: string | null;
  created_at: string;
  updated_at: string;
}

export interface OrderLine {
  id: string;
  product_id: string | null;
  name: string;
  each: Money;
  quantity: number;
}

export interface OrderListFilter {
  after?: string | null;
  limit?: number;
  state?: OrderState;
}

export type OrderState = "waiting" | "paid" | "sent" | "called_off" | "given_back";

export interface OrderSummary {
  id: string;
  number: number;
  state: OrderState;
  email: string;
  total: Money;
  created_at: string;
  updated_at: string;
}

export interface OrderSummaryPage {
  items: OrderSummary[];
  next_cursor: string | null;
}

export interface OrderTransition {
  to: OrderState;
  payment?: PaymentReceiptInput | unknown;
}

export interface PaginationContract {
  style: string;
  default_limit: number;
  max_limit: number;
}

export interface PasswordResetRedeem {
  token: string;
  password: string;
}

export interface PasswordResetRequest {
  email: string;
}

export interface PasswordResetRequested {
  accepted: boolean;
}

export interface PaymentReceiptInput {
  provider: string;
  reference: string;
}

export interface PeopleListFilter {
  after?: string | null;
  limit?: number;
  status?: PersonListFilterStatus;
}

export interface Person {
  id: string;
  site_id: string;
  email: string;
  name: string;
  email_verified: boolean;
}

export type PersonListFilterStatus = "active" | "suspended" | "removed";

export interface PersonPage {
  items: PersonRecord[];
  next_cursor: string | null;
}

export interface PersonRecord {
  id: string;
  site_id: string;
  email: string;
  name: string;
  status: PersonListFilterStatus;
  email_verified: boolean;
  role_ids: string[];
  created_at: string;
  updated_at: string;
}

export interface PortableAssignment {
  content_id: string;
  term_id: string;
  assigned_at: string;
}

export interface PortableBundle {
  manifest: PortableManifest;
  site: PortableSite;
  languages: PortableLanguage[];
  content_types: PortableContentType[];
  terms: PortableTerm[];
  content: PortableContent[];
  revisions: PortableRevision[];
  slug_history: PortableSlugHistory[];
  assignments: PortableAssignment[];
}

export interface PortableContent {
  id: string;
  kind: string;
  language: string;
  slug: string;
  title: string;
  excerpt: string | null;
  body: string;
  fields: Record<string, unknown>;
  status: "draft" | "scheduled" | "published" | "archived";
  scheduled_at: string | null;
  published_at: string | null;
  revision: number;
  created_at: string;
  updated_at: string;
}

export interface PortableContentType {
  kind: string;
  name: string;
  fields: unknown[];
}

export interface PortableCounts {
  languages: number;
  content_types: number;
  terms: number;
  content: number;
  revisions: number;
  slug_history: number;
  assignments: number;
}

export interface PortableImportRequest {
  bundle: PortableBundle;
  strategy: ImportStrategy;
}

export interface PortableLanguage {
  tag: string;
  name: string;
  is_default: boolean;
}

export interface PortableManifest {
  format: string;
  version: number;
  source_site_id: string;
  exported_at: string;
  schema_hash: string;
  counts: PortableCounts;
}

export interface PortableRevision {
  content_id: string;
  revision: number;
  kind: string;
  language: string;
  slug: string;
  title: string;
  excerpt: string | null;
  body: string;
  fields: Record<string, unknown>;
  status: string;
  scheduled_at: string | null;
  published_at: string | null;
  created_at: string;
}

export interface PortableSite {
  name: string;
  timezone: string;
  canonical_url: string | null;
}

export interface PortableSlugHistory {
  content_id: string;
  language: string;
  slug: string;
  created_at: string;
}

export interface PortableTerm {
  id: string;
  kind: "category" | "tag";
  language: string;
  slug: string;
  name: string;
  parent_id: string | null;
}

export interface Product {
  id: string;
  slug: string;
  name: string;
  description: string | null;
  price: Money;
  stock: number;
  on_sale: boolean;
  created_at: string;
  updated_at: string;
}

export interface ProductListFilter {
  after?: string | null;
  limit?: number;
}

export interface ProductPage {
  items: Product[];
  next_cursor: string | null;
}

export interface ProductPrice {
  minor: number;
  currency: string;
}

export interface Progress {
  lesson_id: string;
  completed_at: string;
}

export interface PruneAnalytics {
  raw_days: number;
  aggregate_days: number;
}

export interface PruneReceipt {
  deleted_events: number;
  deleted_aggregates: number;
}

export interface PublicContentQuery {
  language?: string | null;
}

export interface PublicForm {
  slug: string;
  name: string;
  fields: FormField[];
}

export interface PublicProduct {
  id: string;
  slug: string;
  name: string;
  description: string | null;
  price: Money;
  can_be_bought: boolean;
}

export interface PublicProductListFilter {
  after?: string | null;
  limit?: number;
}

export interface PublicProductPage {
  items: PublicProduct[];
  next_cursor: string | null;
}

export interface PublicTermArchiveQuery {
  language?: string | null;
  after?: string | null;
  limit?: number;
}

export type Publication = "draft" | "archived" | Record<string, unknown> | Record<string, unknown>;

export type PublicationInput = "draft" | "publish" | "archive" | Record<string, unknown>;

export type PublicationStatus = "draft" | "scheduled" | "published" | "archived";

export interface ReaderListFilter {
  after?: string | null;
  limit?: number;
  standing?: MailStanding;
}

export interface ReceiveMailProviderEvent {
  provider: string;
  event_id: string;
  delivery_id?: string | null;
  recipient: string;
  kind: MailProviderEventKind;
  bounce_class?: MailBounceClass | unknown;
  provider_reference?: string | null;
  reason?: string | null;
  occurred_at: string;
}

export interface RenderedMail {
  subject: string;
  body: string;
  content_type: MailContentType;
}

export interface ReorderLessons {
  order: string[];
}

export interface ReorderLists {
  order: string[];
}

export interface ReorderModules {
  order: string[];
}

export interface ReplaceContentTerms {
  term_ids: string[];
}

export interface ReplaceCourseInstructor {
  grants: CourseInstructorGrant[];
}

export interface ReplacePersonRoles {
  role_ids: string[];
}

export interface ReplaceRoleGrants {
  grants: Grant[];
}

export type ReportKind = "broken" | "missing" | "wanted";

export interface ReportListFilter {
  after?: string | null;
  limit?: number;
  state?: ReportState;
}

export type ReportState = "open" | "closed";

export type RetryDelivery = Record<string, unknown>;

export interface Role {
  id: string;
  site_id: string;
  name: string;
  grants: Grant[];
  created_at: string;
  protected: boolean;
}

export interface RoleListFilter {
  after?: string | null;
  limit?: number;
}

export interface RolePage {
  items: Role[];
  next_cursor: string | null;
}

export interface RotateCredential {
  expected_version: number;
  values: Record<string, unknown>;
}

export interface RunListFilter {
  after?: string | null;
  limit?: number;
  state?: unknown;
}

export type RunState = "running" | "waiting" | "succeeded" | "failed";

export interface RuntimeManifest {
  protocol: string;
  release: string;
  api_contract_version: string;
  api_contract_hash: string;
  storage_schema_version: number;
  runtime_mode: "fixed_site" | "shard";
  site_id: string;
  pagination: PaginationContract;
}

export interface ScheduleContent {
  at: string;
}

export interface SeenCount {
  seen: number;
}

export interface SendCampaign {
  template_id: string;
  variables?: Record<string, unknown>;
  idempotency_key?: string | null;
}

export interface SendCount {
  enqueued: number;
}

export interface SessionCreated {
  id: string;
  token: string;
  expires_at: string;
}

export interface SetupInput {
  site_name: string;
  email: string;
  name: string;
  password: string;
}

export interface SetupStatus {
  initialized: boolean;
}

export interface SimulateFlow {
  event?: Record<string, unknown>;
}

export interface Simulation {
  steps: SimulationStep[];
}

export interface SimulationStep {
  position: number;
  kind: StepKind;
  config: Record<string, unknown>;
  event: Record<string, unknown>;
}

export interface SiteSettings {
  site_id: string;
  name: string;
  timezone: string;
  canonical_url: string | null;
  mail_sender: MailSender;
  analytics_retention: AnalyticsRetention;
  updated_at: string;
}

export interface StartDesignChange {
  name: string;
}

export type StepKind = "send_mail" | "webhook" | "wait" | "add_to_mail_list";

export interface Student {
  id: string;
  email: string;
  name: string;
  standing: StudentStanding;
  created_at: string;
  updated_at: string;
}

export interface StudentActivationInput {
  email: string;
  invitation_token: string;
  password: string;
}

export interface StudentInvitation {
  student: Student;
  invitation_token: string;
  invitation_expires_at: string;
}

export interface StudentListFilter {
  after?: string | null;
  limit?: number;
  standing?: StudentStanding;
}

export interface StudentLoginInput {
  email: string;
  password: string;
}

export interface StudentPage {
  items: Student[];
  next_cursor: string | null;
}

export interface StudentSessionCreated {
  student: Student;
  token: string;
  expires_at: string;
}

export type StudentStanding = "asked" | "learning" | "stopped";

export interface SubmissionExportFilter {
  after?: string | null;
  limit?: number;
}

export interface SubmissionListFilter {
  after?: string | null;
  limit?: number;
  unread?: boolean;
}

export interface SubmissionPage {
  items: FormSubmission[];
  next_cursor: string | null;
}

export interface SubmissionReceipt {
  id: string;
}

export interface SubmitForm {
  answers: Record<string, unknown>;
}

export interface Term {
  id: string;
  site_id: string;
  kind: TermKind;
  language: string;
  slug: string;
  name: string;
  parent_id: string | null;
  created_at: string;
  updated_at: string;
}

export type TermKind = "category" | "tag";

export type TermList = Term[];

export interface TermListFilter {
  after?: string | null;
  limit?: number;
  kind?: TermKind;
  language?: string | null;
  parent_id?: string | null;
  roots?: boolean;
}

export interface TermPage {
  items: Term[];
  next_cursor: string | null;
}

export interface TrashItem {
  kind: TrashKind;
  id: string;
  label: string;
  deleted_at: string;
}

export type TrashKind = "content" | "file" | "term";

export interface TrashListFilter {
  after?: string | null;
  limit?: number;
  kind?: TrashKind;
}

export interface TrashPage {
  items: TrashItem[];
  next_cursor: string | null;
}

export type Trigger = "content_published" | "form_submitted" | "order_paid" | "order_sent" | "course_enrollment_created" | "course_lesson_completed";

export interface TriggerDescription {
  trigger: Trigger;
  emitted_by: string;
}

export type TriggerList = TriggerDescription[];

export interface UnsubscribeReceipt {
  unsubscribed: boolean;
}

export interface UpdateBoard {
  name?: string | null;
  description?: string | null;
  archived?: boolean | null;
}

export interface UpdateCard {
  title?: string | null;
  description?: string | null;
}

export interface UpdateComment {
  body: string;
}

export interface UpdateContent {
  slug?: string | null;
  title?: string | null;
  excerpt?: string | null;
  body?: string | null;
  fields?: Record<string, unknown> | null;
  publication?: PublicationInput;
}

export interface UpdateCourse {
  title?: string | null;
  about?: string | null;
  state?: CourseState | unknown;
}

export interface UpdateFlow {
  name?: string | null;
  enabled?: boolean | null;
  trigger?: unknown;
  steps?: unknown[] | null;
}

export interface UpdateForm {
  name?: string | null;
  fields?: unknown[] | null;
  open?: boolean | null;
  kept_days?: number | null;
}

export interface UpdateLanguage {
  name?: string | null;
  is_default?: boolean | null;
}

export interface UpdateLesson {
  title?: string | null;
  body?: string | null;
  media_file_id?: string | null;
}

export interface UpdateMailList {
  name?: string | null;
}

export interface UpdateMailTemplate {
  subject?: string | null;
  body?: string | null;
  content_type?: MailContentType | unknown;
}

export interface UpdateModule {
  title?: string | null;
}

export interface UpdatePersonStatus {
  status: PersonListFilterStatus;
}

export interface UpdateProduct {
  name?: string | null;
  description?: string | null;
  price_minor?: number | null;
  stock?: number | null;
  on_sale?: boolean | null;
}

export interface UpdateSiteSettings {
  name?: string | null;
  timezone?: string | null;
  canonical_url?: string | null;
  mail_sender?: MailSenderUpdate;
  analytics_retention?: AnalyticsRetentionUpdate;
}

export interface UpdateStudent {
  name?: string | null;
  standing?: StudentStanding | unknown;
}

export interface UpdateTerm {
  name?: string | null;
  parent_id?: string | null;
}

export interface UploadFileQuery {
  name: string;
  visibility?: FileVisibility;
}

export type VariantPreset = "thumbnail" | "medium" | "large";

export interface MaviOperation {
  method: "get" | "post" | "put" | "patch" | "delete";
  path: string;
  input: { location: "json" | "query" | "raw"; shape: string } | null;
  query: string | null;
  output: string | null;
  outputLocation?: "json" | "raw";
  status: number;
  authentication: string;
  permission: { capability: string; action: string } | null;
}

export const operations = {
  "setup.status": { method: "get", path: "/api/v1/setup", input: null, query: null, output: "SetupStatus", status: 200, authentication: "public", permission: null },
  "setup.initialize": { method: "post", path: "/api/v1/setup", input: { location: "json", shape: "SetupInput" }, query: null, output: "Person", status: 201, authentication: "public", permission: null },
  "auth.session.create": { method: "post", path: "/api/v1/auth/sessions", input: { location: "json", shape: "LoginInput" }, query: null, output: "SessionCreated", status: 201, authentication: "public", permission: null },
  "auth.password_reset.request": { method: "post", path: "/api/v1/auth/password-resets", input: { location: "json", shape: "PasswordResetRequest" }, query: null, output: "PasswordResetRequested", status: 202, authentication: "public", permission: null },
  "auth.password_reset.redeem": { method: "post", path: "/api/v1/auth/password-resets/redeem", input: { location: "json", shape: "PasswordResetRedeem" }, query: null, output: "Empty", status: 204, authentication: "public", permission: null },
  "auth.email_verification.request": { method: "post", path: "/api/v1/auth/email-verifications", input: { location: "json", shape: "EmailVerificationRequest" }, query: null, output: "EmailVerificationRequested", status: 202, authentication: "public", permission: null },
  "auth.email_verification.redeem": { method: "post", path: "/api/v1/auth/email-verifications/redeem", input: { location: "json", shape: "EmailVerificationRedeem" }, query: null, output: "Empty", status: 204, authentication: "public", permission: null },
  "auth.session.revoke": { method: "delete", path: "/api/v1/auth/sessions/current", input: null, query: null, output: "Empty", status: 204, authentication: "account", permission: null },
  "auth.session.current": { method: "get", path: "/api/v1/auth/sessions/current", input: null, query: null, output: "CurrentSession", status: 200, authentication: "account", permission: null },
  "auth.api_key.list": { method: "get", path: "/api/v1/auth/api-keys", input: { location: "query", shape: "ApiKeyListFilter" }, query: null, output: "ApiKeyPage", status: 200, authentication: "account", permission: { capability: "people", action: "view" } },
  "auth.api_key.create": { method: "post", path: "/api/v1/auth/api-keys", input: { location: "json", shape: "CreateApiKey" }, query: null, output: "ApiKeyCreated", status: 201, authentication: "account", permission: { capability: "people", action: "write" } },
  "auth.api_key.revoke": { method: "delete", path: "/api/v1/auth/api-keys/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "people", action: "delete" } },
  "people.list": { method: "get", path: "/api/v1/people", input: { location: "query", shape: "PeopleListFilter" }, query: null, output: "PersonPage", status: 200, authentication: "account_or_assistant", permission: { capability: "people", action: "view" } },
  "people.create": { method: "post", path: "/api/v1/people", input: { location: "json", shape: "CreatePerson" }, query: null, output: "PersonRecord", status: 201, authentication: "account_or_assistant", permission: { capability: "people", action: "write" } },
  "people.status.update": { method: "patch", path: "/api/v1/people/{id}/status", input: { location: "json", shape: "UpdatePersonStatus" }, query: null, output: "PersonRecord", status: 200, authentication: "account_or_assistant", permission: { capability: "people", action: "write" } },
  "people.roles.replace": { method: "put", path: "/api/v1/people/{id}/roles", input: { location: "json", shape: "ReplacePersonRoles" }, query: null, output: "PersonRecord", status: 200, authentication: "account_or_assistant", permission: { capability: "people", action: "write" } },
  "roles.list": { method: "get", path: "/api/v1/roles", input: { location: "query", shape: "RoleListFilter" }, query: null, output: "RolePage", status: 200, authentication: "account_or_assistant", permission: { capability: "people", action: "view" } },
  "roles.create": { method: "post", path: "/api/v1/roles", input: { location: "json", shape: "CreateRole" }, query: null, output: "Role", status: 201, authentication: "account_or_assistant", permission: { capability: "people", action: "write" } },
  "roles.delete": { method: "delete", path: "/api/v1/roles/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "people", action: "delete" } },
  "roles.grants.replace": { method: "put", path: "/api/v1/roles/{id}/grants", input: { location: "json", shape: "ReplaceRoleGrants" }, query: null, output: "Role", status: 200, authentication: "account_or_assistant", permission: { capability: "people", action: "write" } },
  "content.list": { method: "get", path: "/api/v1/content", input: { location: "query", shape: "ContentListFilter" }, query: null, output: "ContentPage", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "view" } },
  "content.read": { method: "get", path: "/api/v1/content/{id}", input: null, query: null, output: "Content", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "view" } },
  "content.create": { method: "post", path: "/api/v1/content", input: { location: "json", shape: "CreateContent" }, query: null, output: "Content", status: 201, authentication: "account_or_assistant", permission: { capability: "content", action: "write" } },
  "content.update": { method: "patch", path: "/api/v1/content/{id}", input: { location: "json", shape: "UpdateContent" }, query: null, output: "Content", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "write" } },
  "content.publish": { method: "post", path: "/api/v1/content/{id}/publish", input: null, query: null, output: "Content", status: 200, authentication: "account_or_assistant", permission: { capability: "publish", action: "write" } },
  "content.schedule": { method: "post", path: "/api/v1/content/{id}/schedule", input: { location: "json", shape: "ScheduleContent" }, query: null, output: "Content", status: 200, authentication: "account_or_assistant", permission: { capability: "publish", action: "write" } },
  "content.archive": { method: "post", path: "/api/v1/content/{id}/archive", input: null, query: null, output: "Content", status: 200, authentication: "account_or_assistant", permission: { capability: "publish", action: "write" } },
  "content.trash": { method: "delete", path: "/api/v1/content/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "trash", action: "delete" } },
  "content.restore": { method: "post", path: "/api/v1/content/{id}/restore", input: null, query: null, output: "Content", status: 200, authentication: "account_or_assistant", permission: { capability: "trash", action: "write" } },
  "content.public_read": { method: "get", path: "/public/v1/content/{slug}", input: { location: "query", shape: "PublicContentQuery" }, query: null, output: "Content", status: 200, authentication: "public", permission: null },
  "content_types.list": { method: "get", path: "/api/v1/content-types", input: { location: "query", shape: "ContentTypeListFilter" }, query: null, output: "ContentTypePage", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "view" } },
  "content_types.upsert": { method: "put", path: "/api/v1/content-types/{kind}", input: { location: "json", shape: "DeclareContentType" }, query: null, output: "ContentType", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "write" } },
  "content_types.delete": { method: "delete", path: "/api/v1/content-types/{kind}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "content", action: "delete" } },
  "content.revisions.list": { method: "get", path: "/api/v1/content/{id}/revisions", input: { location: "query", shape: "ContentRevisionListFilter" }, query: null, output: "ContentRevisionPage", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "view" } },
  "content.revisions.read": { method: "get", path: "/api/v1/content/{id}/revisions/{revision}", input: null, query: null, output: "ContentRevision", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "view" } },
  "content.revisions.restore": { method: "post", path: "/api/v1/content/{id}/revisions/{revision}/restore", input: null, query: null, output: "Content", status: 200, authentication: "account_or_assistant", permission: { capability: "content", action: "write" } },
  "settings.read": { method: "get", path: "/api/v1/settings", input: null, query: null, output: "SiteSettings", status: 200, authentication: "account_or_assistant", permission: { capability: "settings", action: "view" } },
  "settings.update": { method: "patch", path: "/api/v1/settings", input: { location: "json", shape: "UpdateSiteSettings" }, query: null, output: "SiteSettings", status: 200, authentication: "account_or_assistant", permission: { capability: "settings", action: "write" } },
  "languages.list": { method: "get", path: "/api/v1/languages", input: { location: "query", shape: "LanguageListFilter" }, query: null, output: "LanguagePage", status: 200, authentication: "account_or_assistant", permission: { capability: "settings", action: "view" } },
  "languages.create": { method: "post", path: "/api/v1/languages", input: { location: "json", shape: "CreateLanguage" }, query: null, output: "Language", status: 201, authentication: "account_or_assistant", permission: { capability: "settings", action: "write" } },
  "languages.update": { method: "patch", path: "/api/v1/languages/{tag}", input: { location: "json", shape: "UpdateLanguage" }, query: null, output: "Language", status: 200, authentication: "account_or_assistant", permission: { capability: "settings", action: "write" } },
  "languages.delete": { method: "delete", path: "/api/v1/languages/{tag}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "settings", action: "delete" } },
  "taxonomy.terms.list": { method: "get", path: "/api/v1/terms", input: { location: "query", shape: "TermListFilter" }, query: null, output: "TermPage", status: 200, authentication: "account_or_assistant", permission: { capability: "taxonomy", action: "view" } },
  "taxonomy.terms.create": { method: "post", path: "/api/v1/terms", input: { location: "json", shape: "CreateTerm" }, query: null, output: "Term", status: 201, authentication: "account_or_assistant", permission: { capability: "taxonomy", action: "write" } },
  "taxonomy.terms.read": { method: "get", path: "/api/v1/terms/{id}", input: null, query: null, output: "Term", status: 200, authentication: "account_or_assistant", permission: { capability: "taxonomy", action: "view" } },
  "taxonomy.terms.update": { method: "patch", path: "/api/v1/terms/{id}", input: { location: "json", shape: "UpdateTerm" }, query: null, output: "Term", status: 200, authentication: "account_or_assistant", permission: { capability: "taxonomy", action: "write" } },
  "taxonomy.terms.trash": { method: "delete", path: "/api/v1/terms/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "taxonomy", action: "delete" } },
  "taxonomy.public_archive": { method: "get", path: "/public/v1/terms/{kind}/{slug}", input: { location: "query", shape: "PublicTermArchiveQuery" }, query: null, output: "ContentPage", status: 200, authentication: "public", permission: null },
  "taxonomy.content_terms.list": { method: "get", path: "/api/v1/content/{id}/terms", input: null, query: null, output: "TermList", status: 200, authentication: "account_or_assistant", permission: { capability: "taxonomy", action: "view" } },
  "taxonomy.content_terms.replace": { method: "put", path: "/api/v1/content/{id}/terms", input: { location: "json", shape: "ReplaceContentTerms" }, query: null, output: "TermList", status: 200, authentication: "account_or_assistant", permission: { capability: "taxonomy", action: "write" } },
  "taxonomy.term_content.list": { method: "get", path: "/api/v1/terms/{id}/content", input: { location: "query", shape: "ContentTermAssignmentListFilter" }, query: null, output: "ContentTermAssignmentPage", status: 200, authentication: "account_or_assistant", permission: { capability: "taxonomy", action: "view" } },
  "media.files.list": { method: "get", path: "/api/v1/files", input: { location: "query", shape: "FileListFilter" }, query: null, output: "FilePage", status: 200, authentication: "account_or_assistant", permission: { capability: "media", action: "view" } },
  "media.files.upload": { method: "post", path: "/api/v1/files", input: { location: "raw", shape: "FileBytes" }, query: "UploadFileQuery", output: "File", status: 201, authentication: "account_or_assistant", permission: { capability: "media", action: "write" } },
  "media.files.read": { method: "get", path: "/api/v1/files/{id}", input: null, query: null, output: "File", status: 200, authentication: "account_or_assistant", permission: { capability: "media", action: "view" } },
  "media.files.download": { method: "get", path: "/api/v1/files/{id}/content", input: null, query: null, output: "FileBytes", outputLocation: "raw", status: 200, authentication: "account_or_assistant", permission: { capability: "media", action: "view" } },
  "media.files.variants.list": { method: "get", path: "/api/v1/files/{id}/variants", input: { location: "query", shape: "FileVariantListFilter" }, query: null, output: "FileVariantPage", status: 200, authentication: "account_or_assistant", permission: { capability: "media", action: "view" } },
  "media.files.variants.download": { method: "get", path: "/api/v1/files/{id}/variants/{preset}/content", input: null, query: null, output: "FileBytes", outputLocation: "raw", status: 200, authentication: "account_or_assistant", permission: { capability: "media", action: "view" } },
  "media.files.public_download": { method: "get", path: "/public/v1/files/{id}", input: null, query: null, output: "FileBytes", outputLocation: "raw", status: 200, authentication: "public", permission: null },
  "media.files.variants.public_download": { method: "get", path: "/public/v1/files/{id}/variants/{preset}", input: null, query: null, output: "FileBytes", outputLocation: "raw", status: 200, authentication: "public", permission: null },
  "media.files.trash": { method: "delete", path: "/api/v1/files/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "media", action: "delete" } },
  "audit.events.list": { method: "get", path: "/api/v1/audit", input: { location: "query", shape: "AuditListFilter" }, query: null, output: "AuditEventPage", status: 200, authentication: "account_or_assistant", permission: { capability: "audit", action: "view" } },
  "audit.events.read": { method: "get", path: "/api/v1/audit/{id}", input: null, query: null, output: "AuditEvent", status: 200, authentication: "account_or_assistant", permission: { capability: "audit", action: "view" } },
  "trash.items.list": { method: "get", path: "/api/v1/trash", input: { location: "query", shape: "TrashListFilter" }, query: null, output: "TrashPage", status: 200, authentication: "account_or_assistant", permission: { capability: "trash", action: "view" } },
  "trash.items.restore": { method: "post", path: "/api/v1/trash/{kind}/{id}/restore", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "trash", action: "write" } },
  "trash.items.delete_permanently": { method: "delete", path: "/api/v1/trash/{kind}/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "trash", action: "delete" } },
  "design.changes.list": { method: "get", path: "/api/v1/design/changes", input: { location: "query", shape: "DesignChangeListFilter" }, query: null, output: "DesignChangePage", status: 200, authentication: "account_or_assistant", permission: { capability: "design", action: "view" } },
  "design.changes.start": { method: "post", path: "/api/v1/design/changes", input: { location: "json", shape: "StartDesignChange" }, query: null, output: "DesignChange", status: 201, authentication: "account_or_assistant", permission: { capability: "design", action: "write" } },
  "design.changes.read": { method: "get", path: "/api/v1/design/changes/{id}", input: null, query: null, output: "DesignChange", status: 200, authentication: "account_or_assistant", permission: { capability: "design", action: "view" } },
  "design.files.list": { method: "get", path: "/api/v1/design/changes/{id}/files", input: { location: "query", shape: "DesignFileListFilter" }, query: null, output: "DesignFilePage", status: 200, authentication: "account_or_assistant", permission: { capability: "design", action: "view" } },
  "design.files.read": { method: "get", path: "/api/v1/design/changes/{id}/file", input: null, query: "DesignFileQuery", output: "DesignFile", status: 200, authentication: "account_or_assistant", permission: { capability: "design", action: "view" } },
  "design.files.write": { method: "put", path: "/api/v1/design/changes/{id}/file", input: { location: "json", shape: "DesignFileInput" }, query: null, output: "DesignFile", status: 200, authentication: "account_or_assistant", permission: { capability: "design", action: "write" } },
  "design.files.remove": { method: "delete", path: "/api/v1/design/changes/{id}/file", input: null, query: "DesignFileQuery", output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "design", action: "delete" } },
  "design.builds.create": { method: "post", path: "/api/v1/design/changes/{id}/builds", input: null, query: null, output: "DesignBuild", status: 201, authentication: "account_or_assistant", permission: { capability: "design", action: "write" } },
  "design.builds.list": { method: "get", path: "/api/v1/design/changes/{id}/builds", input: { location: "query", shape: "DesignBuildListFilter" }, query: null, output: "DesignBuildPage", status: 200, authentication: "account_or_assistant", permission: { capability: "design", action: "view" } },
  "design.changes.publish": { method: "post", path: "/api/v1/design/changes/{id}/publish", input: null, query: null, output: "DesignChange", status: 200, authentication: "account_or_assistant", permission: { capability: "publish", action: "write" } },
  "design.changes.rollback": { method: "post", path: "/api/v1/design/changes/{id}/rollback", input: null, query: null, output: "DesignChange", status: 200, authentication: "account_or_assistant", permission: { capability: "publish", action: "write" } },
  "design.preview.asset": { method: "get", path: "/preview/v1/design/{build_id}/{path}", input: null, query: null, output: "DesignAsset", status: 200, authentication: "public", permission: null },
  "design.public.asset": { method: "get", path: "/public/v1/site/{path}", input: null, query: null, output: "DesignAsset", status: 200, authentication: "public", permission: null },
  "forms.list": { method: "get", path: "/api/v1/forms", input: { location: "query", shape: "FormListFilter" }, query: null, output: "FormPage", status: 200, authentication: "account_or_assistant", permission: { capability: "forms", action: "view" } },
  "forms.create": { method: "post", path: "/api/v1/forms", input: { location: "json", shape: "CreateForm" }, query: null, output: "Form", status: 201, authentication: "account_or_assistant", permission: { capability: "forms", action: "write" } },
  "forms.read": { method: "get", path: "/api/v1/forms/{id}", input: null, query: null, output: "Form", status: 200, authentication: "account_or_assistant", permission: { capability: "forms", action: "view" } },
  "forms.update": { method: "patch", path: "/api/v1/forms/{id}", input: { location: "json", shape: "UpdateForm" }, query: null, output: "Form", status: 200, authentication: "account_or_assistant", permission: { capability: "forms", action: "write" } },
  "forms.delete": { method: "delete", path: "/api/v1/forms/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "forms", action: "delete" } },
  "forms.submissions.list": { method: "get", path: "/api/v1/forms/{id}/submissions", input: { location: "query", shape: "SubmissionListFilter" }, query: null, output: "SubmissionPage", status: 200, authentication: "account_or_assistant", permission: { capability: "forms", action: "view" } },
  "forms.submissions.export": { method: "get", path: "/api/v1/forms/{id}/submissions/export", input: { location: "query", shape: "SubmissionExportFilter" }, query: null, output: "FormSubmissionExport", status: 200, authentication: "account_or_assistant", permission: { capability: "forms", action: "view" } },
  "forms.submissions.mark_read": { method: "post", path: "/api/v1/forms/{id}/submissions/mark-read", input: null, query: null, output: "SeenCount", status: 200, authentication: "account_or_assistant", permission: { capability: "forms", action: "write" } },
  "forms.submissions.delete": { method: "delete", path: "/api/v1/form-submissions/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "forms", action: "delete" } },
  "forms.public.read": { method: "get", path: "/public/v1/forms/{slug}", input: null, query: null, output: "PublicForm", status: 200, authentication: "public", permission: null },
  "forms.public.submit": { method: "post", path: "/public/v1/forms/{slug}/submissions", input: { location: "json", shape: "SubmitForm" }, query: null, output: "SubmissionReceipt", status: 201, authentication: "public", permission: null },
  "feedback.reports.create": { method: "post", path: "/api/v1/feedback/reports", input: { location: "json", shape: "CreateReport" }, query: null, output: "FeedbackReport", status: 201, authentication: "account_or_assistant", permission: { capability: "feedback", action: "write" } },
  "feedback.reports.list": { method: "get", path: "/api/v1/feedback/reports", input: { location: "query", shape: "ReportListFilter" }, query: null, output: "FeedbackReportPage", status: 200, authentication: "account_or_assistant", permission: { capability: "feedback", action: "view" } },
  "mail.templates.list": { method: "get", path: "/api/v1/mail/templates", input: { location: "query", shape: "MailTemplateListFilter" }, query: null, output: "MailTemplatePage", status: 200, authentication: "account_or_assistant", permission: { capability: "mail", action: "view" } },
  "mail.templates.create": { method: "post", path: "/api/v1/mail/templates", input: { location: "json", shape: "CreateMailTemplate" }, query: null, output: "MailTemplate", status: 201, authentication: "account_or_assistant", permission: { capability: "mail", action: "write" } },
  "mail.templates.read": { method: "get", path: "/api/v1/mail/templates/{id}", input: null, query: null, output: "MailTemplate", status: 200, authentication: "account_or_assistant", permission: { capability: "mail", action: "view" } },
  "mail.templates.update": { method: "patch", path: "/api/v1/mail/templates/{id}", input: { location: "json", shape: "UpdateMailTemplate" }, query: null, output: "MailTemplate", status: 200, authentication: "account_or_assistant", permission: { capability: "mail", action: "write" } },
  "mail.templates.delete": { method: "delete", path: "/api/v1/mail/templates/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "mail", action: "delete" } },
  "mail.templates.preview": { method: "post", path: "/api/v1/mail/templates/{id}/preview", input: { location: "json", shape: "MailTemplatePreview" }, query: null, output: "RenderedMail", status: 200, authentication: "account_or_assistant", permission: { capability: "mail", action: "view" } },
  "mail.lists.list": { method: "get", path: "/api/v1/mail/lists", input: { location: "query", shape: "MailListListFilter" }, query: null, output: "MailListPage", status: 200, authentication: "account_or_assistant", permission: { capability: "mail", action: "view" } },
  "mail.lists.create": { method: "post", path: "/api/v1/mail/lists", input: { location: "json", shape: "CreateMailList" }, query: null, output: "MailList", status: 201, authentication: "account_or_assistant", permission: { capability: "mail", action: "write" } },
  "mail.lists.read": { method: "get", path: "/api/v1/mail/lists/{id}", input: null, query: null, output: "MailList", status: 200, authentication: "account_or_assistant", permission: { capability: "mail", action: "view" } },
  "mail.lists.update": { method: "patch", path: "/api/v1/mail/lists/{id}", input: { location: "json", shape: "UpdateMailList" }, query: null, output: "MailList", status: 200, authentication: "account_or_assistant", permission: { capability: "mail", action: "write" } },
  "mail.lists.delete": { method: "delete", path: "/api/v1/mail/lists/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "mail", action: "delete" } },
  "mail.readers.list": { method: "get", path: "/api/v1/mail/lists/{id}/readers", input: { location: "query", shape: "ReaderListFilter" }, query: null, output: "MailReaderPage", status: 200, authentication: "account_or_assistant", permission: { capability: "mail", action: "view" } },
  "mail.readers.add": { method: "post", path: "/api/v1/mail/lists/{id}/readers", input: { location: "json", shape: "AddReader" }, query: null, output: "MailReaderCreated", status: 201, authentication: "account_or_assistant", permission: { capability: "mail", action: "write" } },
  "mail.readers.delete": { method: "delete", path: "/api/v1/mail/readers/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "mail", action: "delete" } },
  "mail.public.unsubscribe": { method: "post", path: "/public/v1/mail/unsubscribe/{token}", input: null, query: null, output: "UnsubscribeReceipt", status: 200, authentication: "public", permission: null },
  "mail.deliveries.list": { method: "get", path: "/api/v1/mail/deliveries", input: { location: "query", shape: "DeliveryListFilter" }, query: null, output: "MailDeliveryPage", status: 200, authentication: "account_or_assistant", permission: { capability: "mail", action: "view" } },
  "mail.deliveries.enqueue": { method: "post", path: "/api/v1/mail/deliveries", input: { location: "json", shape: "EnqueueDelivery" }, query: null, output: "MailDelivery", status: 202, authentication: "account_or_assistant", permission: { capability: "mail", action: "write" } },
  "mail.deliveries.read": { method: "get", path: "/api/v1/mail/deliveries/{id}", input: null, query: null, output: "MailDelivery", status: 200, authentication: "account_or_assistant", permission: { capability: "mail", action: "view" } },
  "mail.deliveries.retry": { method: "post", path: "/api/v1/mail/deliveries/{id}/retry", input: { location: "json", shape: "RetryDelivery" }, query: null, output: "MailDelivery", status: 202, authentication: "account_or_assistant", permission: { capability: "mail", action: "write" } },
  "mail.deliveries.campaign": { method: "post", path: "/api/v1/mail/lists/{id}/deliveries", input: { location: "json", shape: "SendCampaign" }, query: null, output: "SendCount", status: 202, authentication: "account_or_assistant", permission: { capability: "mail", action: "write" } },
  "mail.provider_events.receive": { method: "post", path: "/internal/v1/mail/provider-events", input: { location: "json", shape: "ReceiveMailProviderEvent" }, query: null, output: "MailProviderEventReceipt", status: 200, authentication: "webhook", permission: null },
  "shop.products.list": { method: "get", path: "/api/v1/shop/products", input: { location: "query", shape: "ProductListFilter" }, query: null, output: "ProductPage", status: 200, authentication: "account_or_assistant", permission: { capability: "shop", action: "view" } },
  "shop.products.create": { method: "post", path: "/api/v1/shop/products", input: { location: "json", shape: "CreateProduct" }, query: null, output: "Product", status: 201, authentication: "account_or_assistant", permission: { capability: "shop", action: "write" } },
  "shop.products.read": { method: "get", path: "/api/v1/shop/products/{id}", input: null, query: null, output: "Product", status: 200, authentication: "account_or_assistant", permission: { capability: "shop", action: "view" } },
  "shop.products.update": { method: "patch", path: "/api/v1/shop/products/{id}", input: { location: "json", shape: "UpdateProduct" }, query: null, output: "Product", status: 200, authentication: "account_or_assistant", permission: { capability: "shop", action: "write" } },
  "shop.products.delete": { method: "delete", path: "/api/v1/shop/products/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "shop", action: "delete" } },
  "shop.public.products.list": { method: "get", path: "/public/v1/shop/products", input: { location: "query", shape: "PublicProductListFilter" }, query: null, output: "PublicProductPage", status: 200, authentication: "public", permission: null },
  "shop.coupons.list": { method: "get", path: "/api/v1/shop/coupons", input: { location: "query", shape: "CouponListFilter" }, query: null, output: "CouponPage", status: 200, authentication: "account_or_assistant", permission: { capability: "shop", action: "view" } },
  "shop.coupons.create": { method: "post", path: "/api/v1/shop/coupons", input: { location: "json", shape: "CreateCoupon" }, query: null, output: "Coupon", status: 201, authentication: "account_or_assistant", permission: { capability: "shop", action: "write" } },
  "shop.coupons.delete": { method: "delete", path: "/api/v1/shop/coupons/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "shop", action: "delete" } },
  "shop.orders.list": { method: "get", path: "/api/v1/shop/orders", input: { location: "query", shape: "OrderListFilter" }, query: null, output: "OrderSummaryPage", status: 200, authentication: "account_or_assistant", permission: { capability: "shop", action: "view" } },
  "shop.orders.read": { method: "get", path: "/api/v1/shop/orders/{id}", input: null, query: null, output: "Order", status: 200, authentication: "account_or_assistant", permission: { capability: "shop", action: "view" } },
  "shop.orders.transition": { method: "post", path: "/api/v1/shop/orders/{id}/transition", input: { location: "json", shape: "OrderTransition" }, query: null, output: "Order", status: 200, authentication: "account_or_assistant", permission: { capability: "shop", action: "write" } },
  "shop.public.orders.checkout": { method: "post", path: "/public/v1/shop/orders", input: { location: "json", shape: "CheckoutInput" }, query: null, output: "CheckoutReceipt", status: 201, authentication: "public", permission: null },
  "courses.list": { method: "get", path: "/api/v1/courses", input: { location: "query", shape: "CourseListFilter" }, query: null, output: "CourseSummaryPage", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "view" } },
  "courses.create": { method: "post", path: "/api/v1/courses", input: { location: "json", shape: "CreateCourse" }, query: null, output: "Course", status: 201, authentication: "account_or_assistant", permission: { capability: "courses", action: "write" } },
  "courses.read": { method: "get", path: "/api/v1/courses/{id}", input: null, query: null, output: "Course", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "view" } },
  "courses.update": { method: "patch", path: "/api/v1/courses/{id}", input: { location: "json", shape: "UpdateCourse" }, query: null, output: "Course", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "write" } },
  "courses.modules.reorder": { method: "put", path: "/api/v1/courses/{id}/modules/order", input: { location: "json", shape: "ReorderModules" }, query: null, output: "Course", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "write" } },
  "courses.modules.create": { method: "post", path: "/api/v1/courses/{id}/modules", input: { location: "json", shape: "CreateModule" }, query: null, output: "Module", status: 201, authentication: "account_or_assistant", permission: { capability: "courses", action: "write" } },
  "courses.modules.read": { method: "get", path: "/api/v1/courses/modules/{id}", input: null, query: null, output: "Module", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "view" } },
  "courses.modules.update": { method: "patch", path: "/api/v1/courses/modules/{id}", input: { location: "json", shape: "UpdateModule" }, query: null, output: "Module", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "write" } },
  "courses.modules.delete": { method: "delete", path: "/api/v1/courses/modules/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "courses", action: "delete" } },
  "courses.lessons.list": { method: "get", path: "/api/v1/courses/modules/{id}/lessons", input: { location: "query", shape: "LessonListFilter" }, query: null, output: "LessonPage", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "view" } },
  "courses.lessons.reorder": { method: "put", path: "/api/v1/courses/modules/{id}/lessons/order", input: { location: "json", shape: "ReorderLessons" }, query: null, output: "Module", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "write" } },
  "courses.lessons.create": { method: "post", path: "/api/v1/courses/modules/{id}/lessons", input: { location: "json", shape: "CreateLesson" }, query: null, output: "Lesson", status: 201, authentication: "account_or_assistant", permission: { capability: "courses", action: "write" } },
  "courses.lessons.update": { method: "patch", path: "/api/v1/courses/lessons/{id}", input: { location: "json", shape: "UpdateLesson" }, query: null, output: "Lesson", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "write" } },
  "courses.lessons.delete": { method: "delete", path: "/api/v1/courses/lessons/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "courses", action: "delete" } },
  "courses.instructors.list": { method: "get", path: "/api/v1/courses/{course_id}/instructors", input: { location: "query", shape: "CourseInstructorListFilter" }, query: null, output: "CourseInstructorPage", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "view" } },
  "courses.instructors.replace": { method: "put", path: "/api/v1/courses/{course_id}/instructors/{person_id}", input: { location: "json", shape: "ReplaceCourseInstructor" }, query: null, output: "CourseInstructor", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "write" } },
  "courses.instructors.delete": { method: "delete", path: "/api/v1/courses/{course_id}/instructors/{person_id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "courses", action: "write" } },
  "courses.students.list": { method: "get", path: "/api/v1/courses/students", input: { location: "query", shape: "StudentListFilter" }, query: null, output: "StudentPage", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "view" } },
  "courses.students.create": { method: "post", path: "/api/v1/courses/students", input: { location: "json", shape: "CreateStudent" }, query: null, output: "StudentInvitation", status: 201, authentication: "account_or_assistant", permission: { capability: "courses", action: "write" } },
  "courses.students.invite": { method: "post", path: "/api/v1/courses/students/{id}/invite", input: null, query: null, output: "StudentInvitation", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "write" } },
  "courses.students.update": { method: "patch", path: "/api/v1/courses/students/{id}", input: { location: "json", shape: "UpdateStudent" }, query: null, output: "Student", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "write" } },
  "courses.enrollments.list": { method: "get", path: "/api/v1/courses/{course_id}/enrollments", input: { location: "query", shape: "EnrollmentListFilter" }, query: null, output: "EnrollmentPage", status: 200, authentication: "account_or_assistant", permission: { capability: "courses", action: "view" } },
  "courses.enrollments.create": { method: "post", path: "/api/v1/courses/{course_id}/enrollments", input: { location: "json", shape: "EnrollStudent" }, query: null, output: "Enrollment", status: 201, authentication: "account_or_assistant", permission: { capability: "courses", action: "write" } },
  "courses.enrollments.delete": { method: "delete", path: "/api/v1/courses/enrollments/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "courses", action: "delete" } },
  "courses.students.activate": { method: "post", path: "/public/v1/courses/students/activate", input: { location: "json", shape: "StudentActivationInput" }, query: null, output: "StudentSessionCreated", status: 201, authentication: "public", permission: null },
  "courses.students.session.create": { method: "post", path: "/public/v1/courses/students/sessions", input: { location: "json", shape: "StudentLoginInput" }, query: null, output: "StudentSessionCreated", status: 201, authentication: "public", permission: null },
  "courses.students.session.revoke": { method: "delete", path: "/student/v1/auth/session", input: null, query: null, output: "Empty", status: 204, authentication: "student", permission: null },
  "learning.courses.list": { method: "get", path: "/student/v1/learning/courses", input: { location: "query", shape: "LearningCourseListFilter" }, query: null, output: "LearningCoursePage", status: 200, authentication: "student", permission: null },
  "learning.course.read": { method: "get", path: "/student/v1/learning/courses/{id}", input: null, query: null, output: "LearningCourseDetail", status: 200, authentication: "student", permission: null },
  "learning.lesson.read": { method: "get", path: "/student/v1/learning/lessons/{id}", input: null, query: null, output: "LearningLesson", status: 200, authentication: "student", permission: null },
  "learning.lesson.media.read": { method: "get", path: "/student/v1/learning/lessons/{id}/media", input: null, query: null, output: "FileBytes", outputLocation: "raw", status: 200, authentication: "student", permission: null },
  "learning.lesson.done": { method: "put", path: "/student/v1/learning/lessons/{id}/done", input: null, query: null, output: "Progress", status: 200, authentication: "student", permission: null },
  "jobs.list": { method: "get", path: "/api/v1/jobs", input: { location: "query", shape: "JobListFilter" }, query: null, output: "JobPage", status: 200, authentication: "account_or_assistant", permission: { capability: "automation", action: "view" } },
  "jobs.read": { method: "get", path: "/api/v1/jobs/{id}", input: null, query: null, output: "Job", status: 200, authentication: "account_or_assistant", permission: { capability: "automation", action: "view" } },
  "jobs.retry": { method: "post", path: "/api/v1/jobs/{id}/retry", input: null, query: null, output: "Job", status: 200, authentication: "account_or_assistant", permission: { capability: "automation", action: "write" } },
  "automation.triggers.list": { method: "get", path: "/api/v1/automation/triggers", input: null, query: null, output: "TriggerList", status: 200, authentication: "account_or_assistant", permission: { capability: "automation", action: "view" } },
  "automation.flows.list": { method: "get", path: "/api/v1/automation/flows", input: { location: "query", shape: "FlowListFilter" }, query: null, output: "FlowPage", status: 200, authentication: "account_or_assistant", permission: { capability: "automation", action: "view" } },
  "automation.flows.create": { method: "post", path: "/api/v1/automation/flows", input: { location: "json", shape: "CreateFlow" }, query: null, output: "Flow", status: 201, authentication: "account_or_assistant", permission: { capability: "automation", action: "write" } },
  "automation.flows.read": { method: "get", path: "/api/v1/automation/flows/{id}", input: null, query: null, output: "Flow", status: 200, authentication: "account_or_assistant", permission: { capability: "automation", action: "view" } },
  "automation.flows.update": { method: "patch", path: "/api/v1/automation/flows/{id}", input: { location: "json", shape: "UpdateFlow" }, query: null, output: "Flow", status: 200, authentication: "account_or_assistant", permission: { capability: "automation", action: "write" } },
  "automation.flows.delete": { method: "delete", path: "/api/v1/automation/flows/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "automation", action: "write" } },
  "automation.flows.simulate": { method: "post", path: "/api/v1/automation/flows/{id}/simulate", input: { location: "json", shape: "SimulateFlow" }, query: null, output: "Simulation", status: 200, authentication: "account_or_assistant", permission: { capability: "automation", action: "view" } },
  "automation.runs.list": { method: "get", path: "/api/v1/automation/flows/{id}/runs", input: { location: "query", shape: "RunListFilter" }, query: null, output: "FlowRunPage", status: 200, authentication: "account_or_assistant", permission: { capability: "automation", action: "view" } },
  "automation.runs.read": { method: "get", path: "/api/v1/automation/runs/{id}", input: null, query: null, output: "FlowRun", status: 200, authentication: "account_or_assistant", permission: { capability: "automation", action: "view" } },
  "boards.list": { method: "get", path: "/api/v1/boards", input: { location: "query", shape: "BoardListFilter" }, query: null, output: "BoardPage", status: 200, authentication: "account_or_assistant", permission: { capability: "boards", action: "view" } },
  "boards.create": { method: "post", path: "/api/v1/boards", input: { location: "json", shape: "CreateBoard" }, query: null, output: "Board", status: 201, authentication: "account_or_assistant", permission: { capability: "boards", action: "write" } },
  "boards.read": { method: "get", path: "/api/v1/boards/{id}", input: null, query: null, output: "Board", status: 200, authentication: "account_or_assistant", permission: { capability: "boards", action: "view" } },
  "boards.update": { method: "patch", path: "/api/v1/boards/{id}", input: { location: "json", shape: "UpdateBoard" }, query: null, output: "Board", status: 200, authentication: "account_or_assistant", permission: { capability: "boards", action: "write" } },
  "boards.delete": { method: "delete", path: "/api/v1/boards/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "boards", action: "delete" } },
  "boards.lists.list": { method: "get", path: "/api/v1/boards/{id}/lists", input: { location: "query", shape: "ListPageFilter" }, query: null, output: "BoardListPage", status: 200, authentication: "account_or_assistant", permission: { capability: "boards", action: "view" } },
  "boards.lists.create": { method: "post", path: "/api/v1/boards/{id}/lists", input: { location: "json", shape: "CreateList" }, query: null, output: "BoardList", status: 201, authentication: "account_or_assistant", permission: { capability: "boards", action: "write" } },
  "boards.lists.reorder": { method: "put", path: "/api/v1/boards/{id}/lists/order", input: { location: "json", shape: "ReorderLists" }, query: null, output: "BoardListPage", status: 200, authentication: "account_or_assistant", permission: { capability: "boards", action: "write" } },
  "boards.cards.list": { method: "get", path: "/api/v1/boards/lists/{id}/cards", input: { location: "query", shape: "CardPageFilter" }, query: null, output: "CardPage", status: 200, authentication: "account_or_assistant", permission: { capability: "boards", action: "view" } },
  "boards.cards.create": { method: "post", path: "/api/v1/boards/lists/{id}/cards", input: { location: "json", shape: "CreateCard" }, query: null, output: "Card", status: 201, authentication: "account_or_assistant", permission: { capability: "boards", action: "write" } },
  "boards.cards.read": { method: "get", path: "/api/v1/boards/cards/{id}", input: null, query: null, output: "Card", status: 200, authentication: "account_or_assistant", permission: { capability: "boards", action: "view" } },
  "boards.cards.update": { method: "patch", path: "/api/v1/boards/cards/{id}", input: { location: "json", shape: "UpdateCard" }, query: null, output: "Card", status: 200, authentication: "account_or_assistant", permission: { capability: "boards", action: "write" } },
  "boards.cards.delete": { method: "delete", path: "/api/v1/boards/cards/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "boards", action: "delete" } },
  "boards.cards.move": { method: "post", path: "/api/v1/boards/cards/{id}/move", input: { location: "json", shape: "MoveCard" }, query: null, output: "Card", status: 200, authentication: "account_or_assistant", permission: { capability: "boards", action: "write" } },
  "boards.cards.assign": { method: "post", path: "/api/v1/boards/cards/{id}/assign", input: { location: "json", shape: "AssignCard" }, query: null, output: "Card", status: 200, authentication: "account_or_assistant", permission: { capability: "boards", action: "write" } },
  "boards.comments.list": { method: "get", path: "/api/v1/boards/cards/{id}/comments", input: { location: "query", shape: "CommentPageFilter" }, query: null, output: "CommentPage", status: 200, authentication: "account_or_assistant", permission: { capability: "boards", action: "view" } },
  "boards.comments.create": { method: "post", path: "/api/v1/boards/cards/{id}/comments", input: { location: "json", shape: "CreateComment" }, query: null, output: "Comment", status: 201, authentication: "account_or_assistant", permission: { capability: "boards", action: "write" } },
  "boards.comments.update": { method: "patch", path: "/api/v1/boards/comments/{id}", input: { location: "json", shape: "UpdateComment" }, query: null, output: "Comment", status: 200, authentication: "account_or_assistant", permission: { capability: "boards", action: "write" } },
  "boards.comments.delete": { method: "delete", path: "/api/v1/boards/comments/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "boards", action: "delete" } },
  "boards.activity.list": { method: "get", path: "/api/v1/boards/{id}/activity", input: { location: "query", shape: "ActivityPageFilter" }, query: null, output: "ActivityPage", status: 200, authentication: "account_or_assistant", permission: { capability: "boards", action: "view" } },
  "analytics.events.ingest": { method: "post", path: "/public/v1/analytics/events", input: { location: "json", shape: "AnalyticsEventBatch" }, query: null, output: "AnalyticsReceipt", status: 202, authentication: "public", permission: null },
  "analytics.events.list": { method: "get", path: "/api/v1/analytics/events", input: { location: "query", shape: "EventListFilter" }, query: null, output: "AnalyticsEventPage", status: 200, authentication: "account_or_assistant", permission: { capability: "analytics", action: "view" } },
  "analytics.daily.list": { method: "get", path: "/api/v1/analytics/daily", input: { location: "query", shape: "DailyListFilter" }, query: null, output: "DailyAggregatePage", status: 200, authentication: "account_or_assistant", permission: { capability: "analytics", action: "view" } },
  "analytics.retention.prune": { method: "post", path: "/api/v1/analytics/prune", input: { location: "json", shape: "PruneAnalytics" }, query: null, output: "PruneReceipt", status: 200, authentication: "account_or_assistant", permission: { capability: "analytics", action: "delete" } },
  "portable.export": { method: "get", path: "/api/v1/portable/export", input: null, query: null, output: "PortableBundle", status: 200, authentication: "account_or_assistant", permission: { capability: "portable", action: "view" } },
  "portable.import": { method: "post", path: "/api/v1/portable/import", input: { location: "json", shape: "PortableImportRequest" }, query: null, output: "ImportReceipt", status: 200, authentication: "account_or_assistant", permission: { capability: "portable", action: "write" } },
  "credentials.list": { method: "get", path: "/api/v1/credentials", input: { location: "query", shape: "CredentialListFilter" }, query: null, output: "CredentialPage", status: 200, authentication: "account_or_assistant", permission: { capability: "credentials", action: "view" } },
  "credentials.create": { method: "post", path: "/api/v1/credentials", input: { location: "json", shape: "CreateCredential" }, query: null, output: "Credential", status: 201, authentication: "account_or_assistant", permission: { capability: "credentials", action: "write" } },
  "credentials.rotate": { method: "put", path: "/api/v1/credentials/{id}", input: { location: "json", shape: "RotateCredential" }, query: null, output: "Credential", status: 200, authentication: "account_or_assistant", permission: { capability: "credentials", action: "write" } },
  "credentials.revoke": { method: "delete", path: "/api/v1/credentials/{id}", input: null, query: null, output: "Empty", status: 204, authentication: "account_or_assistant", permission: { capability: "credentials", action: "delete" } },
  "runtime.manifest.read": { method: "get", path: "/api/v1/runtime/manifest", input: null, query: null, output: "RuntimeManifest", status: 200, authentication: "public", permission: null },
} as const satisfies Record<string, MaviOperation>;

export type OperationName = keyof typeof operations;

export interface OperationArguments {
  "setup.status": { path?: never; query?: never; body?: never; }
  "setup.initialize": { path?: never; query?: never; body: SetupInput; }
  "auth.session.create": { path?: never; query?: never; body: LoginInput; }
  "auth.password_reset.request": { path?: never; query?: never; body: PasswordResetRequest; }
  "auth.password_reset.redeem": { path?: never; query?: never; body: PasswordResetRedeem; }
  "auth.email_verification.request": { path?: never; query?: never; body: EmailVerificationRequest; }
  "auth.email_verification.redeem": { path?: never; query?: never; body: EmailVerificationRedeem; }
  "auth.session.revoke": { path?: never; query?: never; body?: never; }
  "auth.session.current": { path?: never; query?: never; body?: never; }
  "auth.api_key.list": { path?: never; query: ApiKeyListFilter; body?: never; }
  "auth.api_key.create": { path?: never; query?: never; body: CreateApiKey; }
  "auth.api_key.revoke": { path: { id: string }; query?: never; body?: never; }
  "people.list": { path?: never; query: PeopleListFilter; body?: never; }
  "people.create": { path?: never; query?: never; body: CreatePerson; }
  "people.status.update": { path: { id: string }; query?: never; body: UpdatePersonStatus; }
  "people.roles.replace": { path: { id: string }; query?: never; body: ReplacePersonRoles; }
  "roles.list": { path?: never; query: RoleListFilter; body?: never; }
  "roles.create": { path?: never; query?: never; body: CreateRole; }
  "roles.delete": { path: { id: string }; query?: never; body?: never; }
  "roles.grants.replace": { path: { id: string }; query?: never; body: ReplaceRoleGrants; }
  "content.list": { path?: never; query: ContentListFilter; body?: never; }
  "content.read": { path: { id: string }; query?: never; body?: never; }
  "content.create": { path?: never; query?: never; body: CreateContent; }
  "content.update": { path: { id: string }; query?: never; body: UpdateContent; }
  "content.publish": { path: { id: string }; query?: never; body?: never; }
  "content.schedule": { path: { id: string }; query?: never; body: ScheduleContent; }
  "content.archive": { path: { id: string }; query?: never; body?: never; }
  "content.trash": { path: { id: string }; query?: never; body?: never; }
  "content.restore": { path: { id: string }; query?: never; body?: never; }
  "content.public_read": { path: { slug: string }; query: PublicContentQuery; body?: never; }
  "content_types.list": { path?: never; query: ContentTypeListFilter; body?: never; }
  "content_types.upsert": { path: { kind: string }; query?: never; body: DeclareContentType; }
  "content_types.delete": { path: { kind: string }; query?: never; body?: never; }
  "content.revisions.list": { path: { id: string }; query: ContentRevisionListFilter; body?: never; }
  "content.revisions.read": { path: { id: string; revision: string }; query?: never; body?: never; }
  "content.revisions.restore": { path: { id: string; revision: string }; query?: never; body?: never; }
  "settings.read": { path?: never; query?: never; body?: never; }
  "settings.update": { path?: never; query?: never; body: UpdateSiteSettings; }
  "languages.list": { path?: never; query: LanguageListFilter; body?: never; }
  "languages.create": { path?: never; query?: never; body: CreateLanguage; }
  "languages.update": { path: { tag: string }; query?: never; body: UpdateLanguage; }
  "languages.delete": { path: { tag: string }; query?: never; body?: never; }
  "taxonomy.terms.list": { path?: never; query: TermListFilter; body?: never; }
  "taxonomy.terms.create": { path?: never; query?: never; body: CreateTerm; }
  "taxonomy.terms.read": { path: { id: string }; query?: never; body?: never; }
  "taxonomy.terms.update": { path: { id: string }; query?: never; body: UpdateTerm; }
  "taxonomy.terms.trash": { path: { id: string }; query?: never; body?: never; }
  "taxonomy.public_archive": { path: { kind: string; slug: string }; query: PublicTermArchiveQuery; body?: never; }
  "taxonomy.content_terms.list": { path: { id: string }; query?: never; body?: never; }
  "taxonomy.content_terms.replace": { path: { id: string }; query?: never; body: ReplaceContentTerms; }
  "taxonomy.term_content.list": { path: { id: string }; query: ContentTermAssignmentListFilter; body?: never; }
  "media.files.list": { path?: never; query: FileListFilter; body?: never; }
  "media.files.upload": { path?: never; query: UploadFileQuery; body: Blob | ArrayBuffer | Uint8Array; }
  "media.files.read": { path: { id: string }; query?: never; body?: never; }
  "media.files.download": { path: { id: string }; query?: never; body?: never; }
  "media.files.variants.list": { path: { id: string }; query: FileVariantListFilter; body?: never; }
  "media.files.variants.download": { path: { id: string; preset: string }; query?: never; body?: never; }
  "media.files.public_download": { path: { id: string }; query?: never; body?: never; }
  "media.files.variants.public_download": { path: { id: string; preset: string }; query?: never; body?: never; }
  "media.files.trash": { path: { id: string }; query?: never; body?: never; }
  "audit.events.list": { path?: never; query: AuditListFilter; body?: never; }
  "audit.events.read": { path: { id: string }; query?: never; body?: never; }
  "trash.items.list": { path?: never; query: TrashListFilter; body?: never; }
  "trash.items.restore": { path: { kind: string; id: string }; query?: never; body?: never; }
  "trash.items.delete_permanently": { path: { kind: string; id: string }; query?: never; body?: never; }
  "design.changes.list": { path?: never; query: DesignChangeListFilter; body?: never; }
  "design.changes.start": { path?: never; query?: never; body: StartDesignChange; }
  "design.changes.read": { path: { id: string }; query?: never; body?: never; }
  "design.files.list": { path: { id: string }; query: DesignFileListFilter; body?: never; }
  "design.files.read": { path: { id: string }; query: DesignFileQuery; body?: never; }
  "design.files.write": { path: { id: string }; query?: never; body: DesignFileInput; }
  "design.files.remove": { path: { id: string }; query: DesignFileQuery; body?: never; }
  "design.builds.create": { path: { id: string }; query?: never; body?: never; }
  "design.builds.list": { path: { id: string }; query: DesignBuildListFilter; body?: never; }
  "design.changes.publish": { path: { id: string }; query?: never; body?: never; }
  "design.changes.rollback": { path: { id: string }; query?: never; body?: never; }
  "design.preview.asset": { path: { build_id: string; path: string }; query?: never; body?: never; }
  "design.public.asset": { path: { path: string }; query?: never; body?: never; }
  "forms.list": { path?: never; query: FormListFilter; body?: never; }
  "forms.create": { path?: never; query?: never; body: CreateForm; }
  "forms.read": { path: { id: string }; query?: never; body?: never; }
  "forms.update": { path: { id: string }; query?: never; body: UpdateForm; }
  "forms.delete": { path: { id: string }; query?: never; body?: never; }
  "forms.submissions.list": { path: { id: string }; query: SubmissionListFilter; body?: never; }
  "forms.submissions.export": { path: { id: string }; query: SubmissionExportFilter; body?: never; }
  "forms.submissions.mark_read": { path: { id: string }; query?: never; body?: never; }
  "forms.submissions.delete": { path: { id: string }; query?: never; body?: never; }
  "forms.public.read": { path: { slug: string }; query?: never; body?: never; }
  "forms.public.submit": { path: { slug: string }; query?: never; body: SubmitForm; }
  "feedback.reports.create": { path?: never; query?: never; body: CreateReport; }
  "feedback.reports.list": { path?: never; query: ReportListFilter; body?: never; }
  "mail.templates.list": { path?: never; query: MailTemplateListFilter; body?: never; }
  "mail.templates.create": { path?: never; query?: never; body: CreateMailTemplate; }
  "mail.templates.read": { path: { id: string }; query?: never; body?: never; }
  "mail.templates.update": { path: { id: string }; query?: never; body: UpdateMailTemplate; }
  "mail.templates.delete": { path: { id: string }; query?: never; body?: never; }
  "mail.templates.preview": { path: { id: string }; query?: never; body: MailTemplatePreview; }
  "mail.lists.list": { path?: never; query: MailListListFilter; body?: never; }
  "mail.lists.create": { path?: never; query?: never; body: CreateMailList; }
  "mail.lists.read": { path: { id: string }; query?: never; body?: never; }
  "mail.lists.update": { path: { id: string }; query?: never; body: UpdateMailList; }
  "mail.lists.delete": { path: { id: string }; query?: never; body?: never; }
  "mail.readers.list": { path: { id: string }; query: ReaderListFilter; body?: never; }
  "mail.readers.add": { path: { id: string }; query?: never; body: AddReader; }
  "mail.readers.delete": { path: { id: string }; query?: never; body?: never; }
  "mail.public.unsubscribe": { path: { token: string }; query?: never; body?: never; }
  "mail.deliveries.list": { path?: never; query: DeliveryListFilter; body?: never; }
  "mail.deliveries.enqueue": { path?: never; query?: never; body: EnqueueDelivery; }
  "mail.deliveries.read": { path: { id: string }; query?: never; body?: never; }
  "mail.deliveries.retry": { path: { id: string }; query?: never; body: RetryDelivery; }
  "mail.deliveries.campaign": { path: { id: string }; query?: never; body: SendCampaign; }
  "mail.provider_events.receive": { path?: never; query?: never; body: ReceiveMailProviderEvent; }
  "shop.products.list": { path?: never; query: ProductListFilter; body?: never; }
  "shop.products.create": { path?: never; query?: never; body: CreateProduct; }
  "shop.products.read": { path: { id: string }; query?: never; body?: never; }
  "shop.products.update": { path: { id: string }; query?: never; body: UpdateProduct; }
  "shop.products.delete": { path: { id: string }; query?: never; body?: never; }
  "shop.public.products.list": { path?: never; query: PublicProductListFilter; body?: never; }
  "shop.coupons.list": { path?: never; query: CouponListFilter; body?: never; }
  "shop.coupons.create": { path?: never; query?: never; body: CreateCoupon; }
  "shop.coupons.delete": { path: { id: string }; query?: never; body?: never; }
  "shop.orders.list": { path?: never; query: OrderListFilter; body?: never; }
  "shop.orders.read": { path: { id: string }; query?: never; body?: never; }
  "shop.orders.transition": { path: { id: string }; query?: never; body: OrderTransition; }
  "shop.public.orders.checkout": { path?: never; query?: never; body: CheckoutInput; }
  "courses.list": { path?: never; query: CourseListFilter; body?: never; }
  "courses.create": { path?: never; query?: never; body: CreateCourse; }
  "courses.read": { path: { id: string }; query?: never; body?: never; }
  "courses.update": { path: { id: string }; query?: never; body: UpdateCourse; }
  "courses.modules.reorder": { path: { id: string }; query?: never; body: ReorderModules; }
  "courses.modules.create": { path: { id: string }; query?: never; body: CreateModule; }
  "courses.modules.read": { path: { id: string }; query?: never; body?: never; }
  "courses.modules.update": { path: { id: string }; query?: never; body: UpdateModule; }
  "courses.modules.delete": { path: { id: string }; query?: never; body?: never; }
  "courses.lessons.list": { path: { id: string }; query: LessonListFilter; body?: never; }
  "courses.lessons.reorder": { path: { id: string }; query?: never; body: ReorderLessons; }
  "courses.lessons.create": { path: { id: string }; query?: never; body: CreateLesson; }
  "courses.lessons.update": { path: { id: string }; query?: never; body: UpdateLesson; }
  "courses.lessons.delete": { path: { id: string }; query?: never; body?: never; }
  "courses.instructors.list": { path: { course_id: string }; query: CourseInstructorListFilter; body?: never; }
  "courses.instructors.replace": { path: { course_id: string; person_id: string }; query?: never; body: ReplaceCourseInstructor; }
  "courses.instructors.delete": { path: { course_id: string; person_id: string }; query?: never; body?: never; }
  "courses.students.list": { path?: never; query: StudentListFilter; body?: never; }
  "courses.students.create": { path?: never; query?: never; body: CreateStudent; }
  "courses.students.invite": { path: { id: string }; query?: never; body?: never; }
  "courses.students.update": { path: { id: string }; query?: never; body: UpdateStudent; }
  "courses.enrollments.list": { path: { course_id: string }; query: EnrollmentListFilter; body?: never; }
  "courses.enrollments.create": { path: { course_id: string }; query?: never; body: EnrollStudent; }
  "courses.enrollments.delete": { path: { id: string }; query?: never; body?: never; }
  "courses.students.activate": { path?: never; query?: never; body: StudentActivationInput; }
  "courses.students.session.create": { path?: never; query?: never; body: StudentLoginInput; }
  "courses.students.session.revoke": { path?: never; query?: never; body?: never; }
  "learning.courses.list": { path?: never; query: LearningCourseListFilter; body?: never; }
  "learning.course.read": { path: { id: string }; query?: never; body?: never; }
  "learning.lesson.read": { path: { id: string }; query?: never; body?: never; }
  "learning.lesson.media.read": { path: { id: string }; query?: never; body?: never; }
  "learning.lesson.done": { path: { id: string }; query?: never; body?: never; }
  "jobs.list": { path?: never; query: JobListFilter; body?: never; }
  "jobs.read": { path: { id: string }; query?: never; body?: never; }
  "jobs.retry": { path: { id: string }; query?: never; body?: never; }
  "automation.triggers.list": { path?: never; query?: never; body?: never; }
  "automation.flows.list": { path?: never; query: FlowListFilter; body?: never; }
  "automation.flows.create": { path?: never; query?: never; body: CreateFlow; }
  "automation.flows.read": { path: { id: string }; query?: never; body?: never; }
  "automation.flows.update": { path: { id: string }; query?: never; body: UpdateFlow; }
  "automation.flows.delete": { path: { id: string }; query?: never; body?: never; }
  "automation.flows.simulate": { path: { id: string }; query?: never; body: SimulateFlow; }
  "automation.runs.list": { path: { id: string }; query: RunListFilter; body?: never; }
  "automation.runs.read": { path: { id: string }; query?: never; body?: never; }
  "boards.list": { path?: never; query: BoardListFilter; body?: never; }
  "boards.create": { path?: never; query?: never; body: CreateBoard; }
  "boards.read": { path: { id: string }; query?: never; body?: never; }
  "boards.update": { path: { id: string }; query?: never; body: UpdateBoard; }
  "boards.delete": { path: { id: string }; query?: never; body?: never; }
  "boards.lists.list": { path: { id: string }; query: ListPageFilter; body?: never; }
  "boards.lists.create": { path: { id: string }; query?: never; body: CreateList; }
  "boards.lists.reorder": { path: { id: string }; query?: never; body: ReorderLists; }
  "boards.cards.list": { path: { id: string }; query: CardPageFilter; body?: never; }
  "boards.cards.create": { path: { id: string }; query?: never; body: CreateCard; }
  "boards.cards.read": { path: { id: string }; query?: never; body?: never; }
  "boards.cards.update": { path: { id: string }; query?: never; body: UpdateCard; }
  "boards.cards.delete": { path: { id: string }; query?: never; body?: never; }
  "boards.cards.move": { path: { id: string }; query?: never; body: MoveCard; }
  "boards.cards.assign": { path: { id: string }; query?: never; body: AssignCard; }
  "boards.comments.list": { path: { id: string }; query: CommentPageFilter; body?: never; }
  "boards.comments.create": { path: { id: string }; query?: never; body: CreateComment; }
  "boards.comments.update": { path: { id: string }; query?: never; body: UpdateComment; }
  "boards.comments.delete": { path: { id: string }; query?: never; body?: never; }
  "boards.activity.list": { path: { id: string }; query: ActivityPageFilter; body?: never; }
  "analytics.events.ingest": { path?: never; query?: never; body: AnalyticsEventBatch; }
  "analytics.events.list": { path?: never; query: EventListFilter; body?: never; }
  "analytics.daily.list": { path?: never; query: DailyListFilter; body?: never; }
  "analytics.retention.prune": { path?: never; query?: never; body: PruneAnalytics; }
  "portable.export": { path?: never; query?: never; body?: never; }
  "portable.import": { path?: never; query?: never; body: PortableImportRequest; }
  "credentials.list": { path?: never; query: CredentialListFilter; body?: never; }
  "credentials.create": { path?: never; query?: never; body: CreateCredential; }
  "credentials.rotate": { path: { id: string }; query?: never; body: RotateCredential; }
  "credentials.revoke": { path: { id: string }; query?: never; body?: never; }
  "runtime.manifest.read": { path?: never; query?: never; body?: never; }
}

export interface OperationResponses {
  "setup.status": SetupStatus;
  "setup.initialize": Person;
  "auth.session.create": SessionCreated;
  "auth.password_reset.request": PasswordResetRequested;
  "auth.password_reset.redeem": void;
  "auth.email_verification.request": EmailVerificationRequested;
  "auth.email_verification.redeem": void;
  "auth.session.revoke": void;
  "auth.session.current": CurrentSession;
  "auth.api_key.list": ApiKeyPage;
  "auth.api_key.create": ApiKeyCreated;
  "auth.api_key.revoke": void;
  "people.list": PersonPage;
  "people.create": PersonRecord;
  "people.status.update": PersonRecord;
  "people.roles.replace": PersonRecord;
  "roles.list": RolePage;
  "roles.create": Role;
  "roles.delete": void;
  "roles.grants.replace": Role;
  "content.list": ContentPage;
  "content.read": Content;
  "content.create": Content;
  "content.update": Content;
  "content.publish": Content;
  "content.schedule": Content;
  "content.archive": Content;
  "content.trash": void;
  "content.restore": Content;
  "content.public_read": Content;
  "content_types.list": ContentTypePage;
  "content_types.upsert": ContentType;
  "content_types.delete": void;
  "content.revisions.list": ContentRevisionPage;
  "content.revisions.read": ContentRevision;
  "content.revisions.restore": Content;
  "settings.read": SiteSettings;
  "settings.update": SiteSettings;
  "languages.list": LanguagePage;
  "languages.create": Language;
  "languages.update": Language;
  "languages.delete": void;
  "taxonomy.terms.list": TermPage;
  "taxonomy.terms.create": Term;
  "taxonomy.terms.read": Term;
  "taxonomy.terms.update": Term;
  "taxonomy.terms.trash": void;
  "taxonomy.public_archive": ContentPage;
  "taxonomy.content_terms.list": TermList;
  "taxonomy.content_terms.replace": TermList;
  "taxonomy.term_content.list": ContentTermAssignmentPage;
  "media.files.list": FilePage;
  "media.files.upload": File;
  "media.files.read": File;
  "media.files.download": FileBytes;
  "media.files.variants.list": FileVariantPage;
  "media.files.variants.download": FileBytes;
  "media.files.public_download": FileBytes;
  "media.files.variants.public_download": FileBytes;
  "media.files.trash": void;
  "audit.events.list": AuditEventPage;
  "audit.events.read": AuditEvent;
  "trash.items.list": TrashPage;
  "trash.items.restore": void;
  "trash.items.delete_permanently": void;
  "design.changes.list": DesignChangePage;
  "design.changes.start": DesignChange;
  "design.changes.read": DesignChange;
  "design.files.list": DesignFilePage;
  "design.files.read": DesignFile;
  "design.files.write": DesignFile;
  "design.files.remove": void;
  "design.builds.create": DesignBuild;
  "design.builds.list": DesignBuildPage;
  "design.changes.publish": DesignChange;
  "design.changes.rollback": DesignChange;
  "design.preview.asset": DesignAsset;
  "design.public.asset": DesignAsset;
  "forms.list": FormPage;
  "forms.create": Form;
  "forms.read": Form;
  "forms.update": Form;
  "forms.delete": void;
  "forms.submissions.list": SubmissionPage;
  "forms.submissions.export": FormSubmissionExport;
  "forms.submissions.mark_read": SeenCount;
  "forms.submissions.delete": void;
  "forms.public.read": PublicForm;
  "forms.public.submit": SubmissionReceipt;
  "feedback.reports.create": FeedbackReport;
  "feedback.reports.list": FeedbackReportPage;
  "mail.templates.list": MailTemplatePage;
  "mail.templates.create": MailTemplate;
  "mail.templates.read": MailTemplate;
  "mail.templates.update": MailTemplate;
  "mail.templates.delete": void;
  "mail.templates.preview": RenderedMail;
  "mail.lists.list": MailListPage;
  "mail.lists.create": MailList;
  "mail.lists.read": MailList;
  "mail.lists.update": MailList;
  "mail.lists.delete": void;
  "mail.readers.list": MailReaderPage;
  "mail.readers.add": MailReaderCreated;
  "mail.readers.delete": void;
  "mail.public.unsubscribe": UnsubscribeReceipt;
  "mail.deliveries.list": MailDeliveryPage;
  "mail.deliveries.enqueue": MailDelivery;
  "mail.deliveries.read": MailDelivery;
  "mail.deliveries.retry": MailDelivery;
  "mail.deliveries.campaign": SendCount;
  "mail.provider_events.receive": MailProviderEventReceipt;
  "shop.products.list": ProductPage;
  "shop.products.create": Product;
  "shop.products.read": Product;
  "shop.products.update": Product;
  "shop.products.delete": void;
  "shop.public.products.list": PublicProductPage;
  "shop.coupons.list": CouponPage;
  "shop.coupons.create": Coupon;
  "shop.coupons.delete": void;
  "shop.orders.list": OrderSummaryPage;
  "shop.orders.read": Order;
  "shop.orders.transition": Order;
  "shop.public.orders.checkout": CheckoutReceipt;
  "courses.list": CourseSummaryPage;
  "courses.create": Course;
  "courses.read": Course;
  "courses.update": Course;
  "courses.modules.reorder": Course;
  "courses.modules.create": Module;
  "courses.modules.read": Module;
  "courses.modules.update": Module;
  "courses.modules.delete": void;
  "courses.lessons.list": LessonPage;
  "courses.lessons.reorder": Module;
  "courses.lessons.create": Lesson;
  "courses.lessons.update": Lesson;
  "courses.lessons.delete": void;
  "courses.instructors.list": CourseInstructorPage;
  "courses.instructors.replace": CourseInstructor;
  "courses.instructors.delete": void;
  "courses.students.list": StudentPage;
  "courses.students.create": StudentInvitation;
  "courses.students.invite": StudentInvitation;
  "courses.students.update": Student;
  "courses.enrollments.list": EnrollmentPage;
  "courses.enrollments.create": Enrollment;
  "courses.enrollments.delete": void;
  "courses.students.activate": StudentSessionCreated;
  "courses.students.session.create": StudentSessionCreated;
  "courses.students.session.revoke": void;
  "learning.courses.list": LearningCoursePage;
  "learning.course.read": LearningCourseDetail;
  "learning.lesson.read": LearningLesson;
  "learning.lesson.media.read": FileBytes;
  "learning.lesson.done": Progress;
  "jobs.list": JobPage;
  "jobs.read": Job;
  "jobs.retry": Job;
  "automation.triggers.list": TriggerList;
  "automation.flows.list": FlowPage;
  "automation.flows.create": Flow;
  "automation.flows.read": Flow;
  "automation.flows.update": Flow;
  "automation.flows.delete": void;
  "automation.flows.simulate": Simulation;
  "automation.runs.list": FlowRunPage;
  "automation.runs.read": FlowRun;
  "boards.list": BoardPage;
  "boards.create": Board;
  "boards.read": Board;
  "boards.update": Board;
  "boards.delete": void;
  "boards.lists.list": BoardListPage;
  "boards.lists.create": BoardList;
  "boards.lists.reorder": BoardListPage;
  "boards.cards.list": CardPage;
  "boards.cards.create": Card;
  "boards.cards.read": Card;
  "boards.cards.update": Card;
  "boards.cards.delete": void;
  "boards.cards.move": Card;
  "boards.cards.assign": Card;
  "boards.comments.list": CommentPage;
  "boards.comments.create": Comment;
  "boards.comments.update": Comment;
  "boards.comments.delete": void;
  "boards.activity.list": ActivityPage;
  "analytics.events.ingest": AnalyticsReceipt;
  "analytics.events.list": AnalyticsEventPage;
  "analytics.daily.list": DailyAggregatePage;
  "analytics.retention.prune": PruneReceipt;
  "portable.export": PortableBundle;
  "portable.import": ImportReceipt;
  "credentials.list": CredentialPage;
  "credentials.create": Credential;
  "credentials.rotate": Credential;
  "credentials.revoke": void;
  "runtime.manifest.read": RuntimeManifest;
}

export interface MaviClientOptions {
  baseUrl: string;
  token?: string;
  fetch?: typeof globalThis.fetch;
}

export class MaviApiError extends Error {
  readonly status: number;
  readonly payload: ErrorEnvelope | null;

  constructor(status: number, payload: ErrorEnvelope | null) {
    super(payload?.error.message ?? `Mavi request failed with status ${status}`);
    this.status = status;
    this.payload = payload;
  }
}

export class MaviClient {
  private readonly options: MaviClientOptions;
  private readonly baseUrl: string;
  private readonly fetcher: typeof globalThis.fetch;

  constructor(options: MaviClientOptions) {
    this.options = options;
    this.baseUrl = options.baseUrl.replace(/\/+$/, "");
    this.fetcher = options.fetch ?? globalThis.fetch;
  }

  async call<Name extends OperationName>(
    operation: Name,
    args: OperationArguments[Name],
  ): Promise<OperationResponses[Name]> {
    const definition: MaviOperation = operations[operation];
    const values = args as {
      path?: Record<string, string>;
      query?: Record<string, unknown>;
      body?: unknown;
    };
    let path: string = definition.path;
    for (const [name, value] of Object.entries(values.path ?? {})) {
      path = path.replace(`{${name}}`, encodeURIComponent(value));
    }
    const url = new URL(`${this.baseUrl}${path}`);
    for (const [name, value] of Object.entries(values.query ?? {})) {
      if (value !== undefined && value !== null) {
        url.searchParams.set(name, String(value));
      }
    }
    const rawResponse = definition.outputLocation === "raw";
    const headers: Record<string, string> = {
      Accept: rawResponse ? "application/octet-stream" : "application/json",
    };
    if (this.options.token) {
      headers.Authorization = `Bearer ${this.options.token}`;
    }
    const rawBody = definition.input?.location === "raw";
    let requestBody: BodyInit | undefined;
    if (values.body !== undefined) {
      headers["Content-Type"] = rawBody
        ? "application/octet-stream"
        : "application/json";
      requestBody = rawBody
        ? (values.body as BodyInit)
        : JSON.stringify(values.body);
    }
    const response = await this.fetcher(url, {
      method: definition.method.toUpperCase(),
      headers,
      body: requestBody,
    });
    if (!response.ok) {
      let payload: ErrorEnvelope | null = null;
      try {
        payload = (await response.json()) as ErrorEnvelope;
      } catch {
        payload = null;
      }
      throw new MaviApiError(response.status, payload);
    }
    if (response.status === 204) {
      return undefined as OperationResponses[Name];
    }
    return (rawResponse
      ? await response.blob()
      : await response.json()) as OperationResponses[Name];
  }
}
