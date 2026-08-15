// Written from the description of the API. Do not edit by hand:
// `UPDATE_SNAPSHOTS=1 cargo test --test api` writes it.

export interface Page<T> {
  items: T[];
  next?: string | null;
}

export interface About {
  email: Email;
}

export interface Accepted {
  id: string;
}

export interface ActOnMany {
  act: string;
  ids: string[];
}

export interface Acted {
  acted_on: number;
  left_alone: string[];
}

export interface Address {
  email: Email;
}

export interface Answers {
  answers: Record<string, unknown>;
}

export interface Arrival {
  token: Shown;
  expires_at: string;
  user: Me;
  redirect: string;
}

export interface Attachment {
  term_ids: string[];
}

export interface Beacon {
  path: string;
  lcp?: number | null;
  inp?: number | null;
  cls?: number | null;
  ttfb?: number | null;
}

export interface Begun {
  secret: Shown;
  uri: string;
}

export interface Board {
  id: string;
  name: string;
  created_at: string;
}

export interface BoardChanges {
  name: Title;
}

export interface Bought {
  name: string;
  quantity: number;
  each: Money;
}

export interface Bundle {
  version: number;
  languages?: BundledLanguage[];
  terms?: BundledTerm[];
  posts?: BundledPost[];
}

export interface BundledLanguage {
  code: string;
  name: string;
  is_default: boolean;
}

export interface BundledPost {
  id: string;
  kind: string;
  state: string;
  language: string;
  slug: string;
  title: string;
  excerpt?: string | null;
  body: string;
  fields?: unknown;
  terms?: string[];
}

export interface BundledTerm {
  id: string;
  kind: string;
  language: string;
  slug: string;
  name: string;
  description?: string | null;
}

export interface Campaign {
  id: string;
  list_id: string;
  subject: string;
  state: CampaignState;
  sent_count: number;
  created_at: string;
}

export type CampaignState = "draft" | "sending" | "sent" | "cancelled";

export interface Card {
  id: string;
  stage_id: string;
  title: string;
  detail?: string | null;
  owner_id?: string | null;
  value?: null | Money;
  position: number;
  created_at: string;
}

export interface CardChanges {
  title?: null | Title;
  detail?: string | null;
  stage_id?: string | null;
  position?: number | null;
  owner_id?: string | null;
}

export interface Change {
  path: string;
  kind: string;
}

export interface Changing {
  current: Secret_String;
  next: Secret_String;
}

export interface Check {
  what: string;
  well: boolean;
  detail: unknown;
}

export interface Checkout {
  email: Email;
  items: Wanted[];
  coupon?: string | null;
  idempotency_key: string;
}

export interface Chosen {
  token: Secret_String;
  password: Secret_String;
}

export interface Configuration {
  label: string;
  client_id: string;
  client_secret: Secret_String;
  authorize_url: string;
  token_url: string;
  profile_url: string;
  scope?: string;
  enabled?: boolean;
}

export interface ContentType {
  id: string;
  key: string;
  name: string;
  plural?: string | null;
  names: unknown;
  fields: unknown;
  posts: number;
}

export interface Copied {
  email: string;
  found: unknown;
}

export interface Count {
  on_day: string;
  count: number;
}

export interface Counts {
  draft: number;
  scheduled: number;
  published: number;
  archived: number;
}

export interface Coupon {
  id: string;
  code: string;
  kind: string;
  value: number;
  uses_allowed?: number | null;
  minimum_minor: number;
  per_shopper?: number | null;
  currency: Currency;
  used: number;
  expires_at?: string | null;
  created_at: string;
}

export interface Course {
  id: string;
  slug: string;
  title: string;
  summary?: string | null;
  state: CourseState;
  created_at: string;
}

export interface CourseChanges {
  title?: null | Title;
  summary?: string | null;
  state?: string | null;
}

export type CourseState = "draft" | "open" | "closed";

export interface Credentials {
  email: Email;
  password: Secret_String;
  code?: string | null;
}

export type Currency = "TRY" | "EUR" | "USD" | "GBP";

export interface Curriculum {
  course: Course;
  modules: Module[];
}

export interface Day {
  on_day: string;
  views: number;
  visitors: number;
}

export interface Design {
  changed: Change[];
  preview?: null | Publish;
  preview_at?: string | null;
  live?: null | Publish;
  building?: null | Publish;
}

export interface Digits {
  code: string;
}

export type Email = string;

export interface Enrolled {
  student_id: string;
  token: Shown;
}

export interface Enrolling {
  days?: number | null;
  email: Email;
  name: Title;
}

export interface Enrolment {
  id: string;
  course_id: string;
  course: string;
  ends_at?: string | null;
  state: string;
  created_at: string;
}

export interface EnrolmentChanges {
  days?: number | null;
  forever?: boolean | null;
}

export interface Entry {
  id: string;
  actor_id?: string | null;
  actor_kind: string;
  actor_name?: string | null;
  action: string;
  subject: string;
  subject_id?: string | null;
  before?: unknown;
  after?: unknown;
  request_id: string;
  created_at: string;
}

export type FieldKind = "text" | "number" | "boolean" | "moment" | "choice";

export interface File {
  path: string;
  branch: string;
  updated_at: string;
}

export interface Finished {
  reference: string;
  ok: boolean;
  seconds?: number | null;
  plays?: unknown;
  note?: string | null;
}

export interface First {
  email: Email;
  name: Title;
  password: Secret_String;
}

export interface Flow {
  id: string;
  name: string;
  trigger: string;
  active: boolean;
  created_at: string;
}

export interface FlowChanges {
  name?: null | Title;
  active?: boolean | null;
  steps?: NewStep[] | null;
}

export interface FlowStep {
  id: string;
  kind: StepKind;
  config: unknown;
  position: number;
}

export interface Form {
  id: string;
  slug: string;
  name: string;
  fields: unknown;
  active: boolean;
  retention_days: number;
  submissions: number;
  unseen: number;
  created_at: string;
}

export interface FormChanges {
  name?: null | Title;
  fields?: FormField[] | null;
  active?: boolean | null;
  retention_days?: number | null;
}

export interface FormField {
  key: Slug;
  label: Title;
  required: boolean;
  kind?: FormFieldKind;
  options?: string[];
}

export type FormFieldKind = "text" | "long" | "email" | "number" | "choice" | "boolean";

export interface Full {
  board: Board;
  stages: Stage[];
}

export interface Handover {
  id: string;
  token: Shown;
  expires_at: string;
  grants: string[];
}

export interface Health {
  well: boolean;
  checks: Check[];
}

export interface Heard {
  provider_ref: string;
  kind: string;
  detail?: string | null;
}

export interface Invitation {
  email: Email;
  name: Title;
  role_id: string;
}

export interface Invited {
  id: string;
  expires_at: string;
}

export interface Issue {
  post_id: string;
  title: string;
  kind: string;
  weight: string;
  detail: unknown;
  written_at: string;
}

export interface Key {
  id: string;
  created_at: string;
  expires_at: string;
  revoked: boolean;
}

export type Kind = "post" | "page";

export interface Language {
  id: string;
  code: string;
  name: string;
  is_default: boolean;
  posts: number;
}

export interface LanguageChanges {
  name?: null | Title;
  is_default?: boolean | null;
}

export interface Learner {
  id: string;
  email: string;
  name: string;
}

export interface Leaving {
  token: Secret_String;
}

export interface LeavingForProvider {
  redirect?: string | null;
  redirect_uri: string;
}

export interface Lesson {
  id: string;
  title: string;
  position: number;
  video_id?: string | null;
  done: boolean;
}

export interface LessonChanges {
  title?: null | Title;
  position?: number | null;
  body?: string | null;
  video_id?: string | null;
}

export interface Letter {
  kind: string;
  language: string;
  subject: string;
  body: string;
  theirs: boolean;
  names: string[];
}

export interface Mail {
  sent: number;
  failed: number;
  bounced: number;
  complained: number;
}

export interface MailList {
  id: string;
  name: string;
  created_at: string;
}

export interface Me {
  id: string;
  email: string;
  name: string;
  role: string;
  grants: string[];
  site: string;
  site_state: string;
}

export interface Media {
  id: string;
  original_name: string;
  mime: string;
  bytes: number;
  created_at: string;
}

export interface Module {
  id: string;
  title: string;
  position: number;
  lessons: Lesson[];
}

export interface ModuleChanges {
  title?: null | Title;
  position?: number | null;
}

export interface Money {
  minor: number;
  currency: Currency;
}

export interface NewBoard {
  name: Title;
  stages?: Title[];
}

export interface NewCampaign {
  list_id: string;
  subject: Title;
  body: string;
}

export interface NewCard {
  stage_id: string;
  title: Title;
  detail?: string | null;
  owner_id?: string | null;
  value_minor?: number | null;
  currency?: null | Currency;
}

export interface NewCoupon {
  code: string;
  kind: string;
  value: number;
  uses_allowed?: number | null;
  minimum_minor?: number | null;
  per_shopper?: number | null;
  currency?: null | Currency;
  expires_at?: string | null;
}

export interface NewCourse {
  slug: Slug;
  title: Title;
  summary?: string | null;
}

export interface NewCredential {
  name: Title;
  secret: Secret_String;
}

export interface NewFlow {
  name: Title;
  trigger: string;
  steps?: NewStep[];
}

export interface NewForm {
  slug: Slug;
  name: Title;
  fields?: FormField[];
  retention_days?: number | null;
}

export interface NewLanguage {
  code: string;
  name: Title;
  is_default?: boolean;
}

export interface NewLesson {
  title: Title;
  position?: number | null;
  body?: string | null;
  video_id?: string | null;
}

export interface NewList {
  name: Title;
}

export interface NewModule {
  title: Title;
  position?: number | null;
}

export interface NewNote {
  body: string;
}

export interface NewPost {
  language: string;
  title: Title;
  slug?: null | Slug;
  kind?: null | Kind;
  excerpt?: string | null;
  body?: string | null;
  fields?: Record<string, unknown> | null;
  type?: string | null;
  cover_media_id?: string | null;
  seo_title?: string | null;
  seo_description?: string | null;
  canonical?: string | null;
  translation_of?: string | null;
}

export interface NewProduct {
  slug: Slug;
  name: Title;
  description?: string | null;
  price_minor: number;
  currency: Currency;
  stock?: number | null;
  low_stock_at?: number | null;
}

export interface NewRole {
  key: string;
  name: Title;
  grants: string[];
}

export interface NewStep {
  kind: StepKind;
  config?: Record<string, unknown>;
}

export interface NewSubscriber {
  email: Email;
  name?: string | null;
}

export interface NewTerm {
  kind: TermKind;
  language: string;
  name: Title;
  slug?: null | Slug;
  description?: string | null;
  parent_id?: string | null;
}

export interface NewType {
  plural?: null | Title;
  names?: unknown;
  key?: string | null;
  name: Title;
  fields: TypeField[];
}

export interface NewVideo {
  media_id: string;
  title: Title;
}

export interface Note {
  id: string;
  author_id?: string | null;
  body: string;
  created_at: string;
}

export interface Offered {
  key: string;
  label: string;
}

export interface OnCourse {
  student_id: string;
  email: string;
  name: string;
  enrolled_at: string;
}

export interface Order {
  id: string;
  number: number;
  state: OrderState;
  email: string;
  total: Money;
  created_at: string;
}

export type OrderState = "pending" | "paid" | "fulfilled" | "cancelled" | "refunded";

export interface Overview {
  days: number;
  totals: Totals;
  written: Count[];
  arrived: Count[];
  visitors: Day[];
  mail: Mail;
}

export interface PageRead {
  path: string;
  views: number;
  visitors: number;
}

export interface Password {
  password: Secret_String;
}

export interface Person {
  id: string;
  email: string;
  name: string;
  role: string;
  state: PersonState;
  email_proved: boolean;
  created_at: string;
}

export interface PersonChanges {
  name?: null | Title;
  email?: null | Email;
  role_id?: string | null;
  suspended?: boolean | null;
}

export type PersonState = "invited" | "active" | "suspended";

export interface Placed {
  order: Order;
  pay_at?: string | null;
}

export interface Plugged {
  key: string;
  configured: boolean;
  enabled: boolean;
  settings: unknown;
  holds: string[];
  working?: boolean | null;
  note?: string | null;
}

export interface Plugging {
  settings: unknown;
  enabled?: boolean;
}

export interface Post {
  id: string;
  kind: Kind;
  state: State;
  language: string;
  slug: string;
  title: string;
  excerpt?: string | null;
  body: string;
  fields: unknown;
  type?: string | null;
  author_id?: string | null;
  cover_media_id?: string | null;
  seo_title?: string | null;
  seo_description?: string | null;
  canonical?: string | null;
  translation_of?: string | null;
  published_at?: string | null;
  created_at: string;
}

export interface PostChanges {
  title?: null | Title;
  slug?: null | Slug;
  excerpt?: string | null;
  body?: string | null;
  fields?: Record<string, unknown> | null;
  type?: string | null;
  state?: null | State;
  publish_at?: string | null;
  cover_media_id?: string | null;
  seo_title?: string | null;
  seo_description?: string | null;
  canonical?: string | null;
}

export interface Product {
  id: string;
  slug: string;
  name: string;
  description?: string | null;
  price: Money;
  stock: number;
  active: boolean;
  created_at: string;
}

export interface ProductChanges {
  name?: null | Title;
  description?: string | null;
  price_minor?: number | null;
  stock?: number | null;
  low_stock_at?: number | null;
  active?: boolean | null;
}

export interface Publish {
  id: string;
  branch: string;
  state: PublishState;
  seconds?: number | null;
  log?: string | null;
  created_at: string;
}

export type PublishState = "queued" | "building" | "live" | "failed" | "cancelled" | "previewed";

export interface Read {
  languages: number;
  terms: number;
  posts: number;
  left_alone: number;
}

export interface Recovery {
  codes: string[];
}

export interface Report {
  id: string;
  kind: ReportKind;
  environment: unknown;
  media_id?: string | null;
  screen?: string | null;
  body: string;
  state: ReportState;
  answer?: string | null;
  answered_at?: string | null;
  created_at: string;
}

export type ReportKind = "broken" | "missing" | "wanted";

export type ReportState = "said" | "seen" | "answered" | "closed";

export interface Request {
  method: string;
  params?: unknown;
}

export interface Returned {
  code: string;
  state: string;
  redirect_uri: string;
  second_factor?: string | null;
}

export interface Role {
  id: string;
  key: string;
  name: string;
  grants: string[];
  built_in: boolean;
}

export interface RoleChanges {
  name?: null | Title;
  grants?: string[] | null;
}

export interface Run {
  id: string;
  state: string;
  at_step: number;
  failure?: string | null;
  started_at: string;
}

export interface Saying {
  kind?: string;
  environment?: unknown;
  media_id?: string | null;
  screen?: string | null;
  body: string;
}

export interface Score {
  kind: string;
  p75: number;
  samples: number;
  verdict: string;
}

export type Secret_String = string;

export interface Seen {
  seen: number;
}

export interface Sent {
  url: string;
  state: string;
}

export interface Session {
  token: Shown;
  expires_at: string;
  user: Me;
}

export interface Settings {
  name: string;
  storage_used_bytes: number;
  storage_limit_bytes?: number | null;
}

export interface SettingsChanges {
  name: Title;
}

export type Shown = string;

export interface Slow {
  path: string;
  lcp?: number | null;
  inp?: number | null;
  cls?: number | null;
  ttfb?: number | null;
  samples: number;
}

export type Slug = string;

export interface Stage {
  id: string;
  name: string;
  position: number;
  cards: Card[];
}

export interface Standing {
  enabled: boolean;
  recovery_codes_left?: number | null;
}

export type State = "draft" | "scheduled" | "published" | "archived";

export type StepKind = "send_mail" | "call_webhook" | "wait" | "add_to_list";

export interface Student {
  id: string;
  email: string;
  name: string;
  state: StudentState;
  last_seen_at?: string | null;
  courses: number;
  created_at: string;
}

export interface StudentChanges {
  name?: null | Title;
  suspended?: boolean | null;
}

export interface StudentCredentials {
  email: Email;
  password: Secret_String;
}

export type StudentState = "invited" | "active" | "suspended";

export interface Submission {
  id: string;
  form_id: string;
  answers: unknown;
  seen_at?: string | null;
  created_at: string;
}

export interface Subscriber {
  id: string;
  email: string;
  name?: string | null;
  state: SubscriberState;
  created_at: string;
}

export type SubscriberState = "subscribed" | "unsubscribed" | "bounced" | "complained";

export interface Summary {
  days: Day[];
  pages: PageRead[];
}

export interface Taken {
  position: number;
  kind?: null | StepKind;
  outcome: string;
  detail: unknown;
  created_at: string;
}

export interface Term {
  id: string;
  kind: TermKind;
  language: string;
  slug: string;
  name: string;
  description?: string | null;
  parent_id?: string | null;
  created_at: string;
}

export interface TermChanges {
  name?: null | Title;
  slug?: null | Slug;
  description?: string | null;
  parent_id?: string | null;
}

export type TermKind = "category" | "tag";

export interface Thrown {
  kind: string;
  id: string;
  name: string;
  thrown_at: string;
  goes_at: string;
}

export type Title = string;

export interface Totals {
  posts: number;
  media: number;
  media_bytes: number;
  forms: number;
  submissions: number;
  unseen: number;
  subscribers: number;
  students: number;
  flows: number;
  flows_on: number;
  orders: number;
}

export interface Translation {
  id: string;
  language: string;
  title: string;
  state: State;
}

export interface TypeField {
  name: string;
  label?: string | null;
  kind: FieldKind;
  required?: boolean;
  choices?: string[];
}

export interface Video {
  id: string;
  media_id?: string | null;
  title: string;
  state: VideoState;
  seconds?: number | null;
  plays: unknown;
  note?: string | null;
  created_at: string;
}

export type VideoState = "waiting" | "working" | "ready" | "failed";

export interface Vitals {
  days: number;
  scores: Score[];
  worst: Slow[];
}

export interface Waiting {
  needed: boolean;
}

export interface Wanted {
  product_id: string;
  quantity: number;
}

export interface Watching {
  id: string;
  title: string;
  body: string;
  course_id: string;
  course: string;
  position: number;
  total: number;
  previous?: string | null;
  next?: string | null;
  video_id?: string | null;
  done: boolean;
}

export interface Whole {
  order: Order;
  lines: Bought[];
}

export interface WholeFlow {
  flow: Flow;
  steps: FlowStep[];
}

export interface WholePost {
  post: Post;
  term_ids: string[];
  translations: Translation[];
}

export interface WholeRun {
  run: Run;
  steps: Taken[];
}

export interface Wording {
  language: string;
  subject: string;
  body: string;
}

export interface Writing {
  path: string;
  body: string;
  branch?: string | null;
}

export interface Written {
  path: string;
  branch: string;
  body: string;
  updated_at: string;
}

export interface Calls {
  "DELETE /api/assistant/keys/{id}": { takes: never; gives: void };
  "DELETE /api/auth/oauth/{key}": { takes: never; gives: void };
  "DELETE /api/auth/second-factor": { takes: {
  password: Secret_String;
}; gives: void };
  "DELETE /api/auth/session": { takes: never; gives: void };
  "DELETE /api/boards/{id}": { takes: never; gives: void };
  "DELETE /api/cards/{id}": { takes: never; gives: void };
  "DELETE /api/content-types/{key}": { takes: never; gives: void };
  "DELETE /api/coupons/{code}": { takes: never; gives: void };
  "DELETE /api/enrolments/{id}": { takes: never; gives: void };
  "DELETE /api/flows/credentials/{name}": { takes: never; gives: void };
  "DELETE /api/flows/{id}": { takes: never; gives: void };
  "DELETE /api/forms/{id}": { takes: never; gives: void };
  "DELETE /api/forms/{id}/submissions/{submission_id}": { takes: never; gives: void };
  "DELETE /api/languages/{code}": { takes: never; gives: void };
  "DELETE /api/learn/session": { takes: never; gives: void };
  "DELETE /api/lessons/{id}": { takes: never; gives: void };
  "DELETE /api/mail/letters/{kind}": { takes: never; gives: void };
  "DELETE /api/media/{id}": { takes: never; gives: void };
  "DELETE /api/modules/{id}": { takes: never; gives: void };
  "DELETE /api/people/{id}": { takes: never; gives: void };
  "DELETE /api/plugins/{key}": { takes: never; gives: void };
  "DELETE /api/posts/{id}": { takes: never; gives: void };
  "DELETE /api/roles/{id}": { takes: never; gives: void };
  "DELETE /api/terms/{id}": { takes: never; gives: void };
  "DELETE /api/trash/{kind}/{id}": { takes: never; gives: void };
  "DELETE /api/videos/{id}": { takes: never; gives: void };
  "GET /api/analytics": { takes: never; gives: {
  days: Day[];
  pages: PageRead[];
} };
  "GET /api/analytics/vitals": { takes: never; gives: {
  days: number;
  scores: Score[];
  worst: Slow[];
} };
  "GET /api/assistant/keys": { takes: never; gives: Page<Key> };
  "GET /api/audit": { takes: never; gives: Page<Entry> };
  "GET /api/audit/export": { takes: never; gives: void };
  "GET /api/auth/me": { takes: never; gives: {
  id: string;
  email: string;
  name: string;
  role: string;
  grants: string[];
  site: string;
  site_state: string;
} };
  "GET /api/auth/oauth": { takes: never; gives: ({
  key: string;
  label: string;
})[] };
  "GET /api/auth/second-factor": { takes: never; gives: {
  enabled: boolean;
  recovery_codes_left?: number | null;
} };
  "GET /api/boards": { takes: never; gives: Page<Board> };
  "GET /api/boards/{id}": { takes: never; gives: {
  board: Board;
  stages: Stage[];
} };
  "GET /api/boards/{id}/cards": { takes: never; gives: Page<Card> };
  "GET /api/cards/{id}": { takes: never; gives: {
  id: string;
  stage_id: string;
  title: string;
  detail?: string | null;
  owner_id?: string | null;
  value?: null | Money;
  position: number;
  created_at: string;
} };
  "GET /api/cards/{id}/notes": { takes: never; gives: Page<Note> };
  "GET /api/content-types": { takes: never; gives: ({
  id: string;
  key: string;
  name: string;
  plural?: string | null;
  names: unknown;
  fields: unknown;
  posts: number;
})[] };
  "GET /api/coupons": { takes: never; gives: Page<Coupon> };
  "GET /api/courses": { takes: never; gives: Page<Course> };
  "GET /api/courses/{id}": { takes: never; gives: {
  course: Course;
  modules: Module[];
} };
  "GET /api/courses/{id}/students": { takes: never; gives: Page<OnCourse> };
  "GET /api/design": { takes: never; gives: {
  changed: Change[];
  preview?: null | Publish;
  preview_at?: string | null;
  live?: null | Publish;
  building?: null | Publish;
} };
  "GET /api/design/file": { takes: never; gives: {
  path: string;
  branch: string;
  body: string;
  updated_at: string;
} };
  "GET /api/design/files": { takes: never; gives: ({
  path: string;
  branch: string;
  updated_at: string;
})[] };
  "GET /api/design/previews": { takes: never; gives: Page<Publish> };
  "GET /api/design/publishes": { takes: never; gives: Page<Publish> };
  "GET /api/domains": { takes: never; gives: ({
  host: string;
  is_primary: boolean;
  resolves?: boolean | null;
  answered?: boolean | null;
  note?: string | null;
  checked_at?: string | null;
})[] };
  "GET /api/flows": { takes: never; gives: Page<Flow> };
  "GET /api/flows/credentials": { takes: never; gives: ({
  name: string;
  updated_at: string;
})[] };
  "GET /api/flows/runs/{id}": { takes: never; gives: {
  run: Run;
  steps: Taken[];
} };
  "GET /api/flows/{id}": { takes: never; gives: {
  flow: Flow;
  steps: FlowStep[];
} };
  "GET /api/flows/{id}/runs": { takes: never; gives: Page<Run> };
  "GET /api/forms": { takes: never; gives: Page<Form> };
  "GET /api/forms/{id}": { takes: never; gives: {
  id: string;
  slug: string;
  name: string;
  fields: unknown;
  active: boolean;
  retention_days: number;
  submissions: number;
  unseen: number;
  created_at: string;
} };
  "GET /api/forms/{id}/submissions": { takes: never; gives: Page<Submission> };
  "GET /api/health": { takes: never; gives: {
  well: boolean;
  checks: Check[];
} };
  "GET /api/languages": { takes: never; gives: ({
  id: string;
  code: string;
  name: string;
  is_default: boolean;
  posts: number;
})[] };
  "GET /api/learn/courses": { takes: never; gives: Page<Course> };
  "GET /api/learn/courses/{id}": { takes: never; gives: {
  course: Course;
  modules: Module[];
} };
  "GET /api/learn/lessons/{id}": { takes: never; gives: {
  id: string;
  title: string;
  body: string;
  course_id: string;
  course: string;
  position: number;
  total: number;
  previous?: string | null;
  next?: string | null;
  video_id?: string | null;
  done: boolean;
} };
  "GET /api/learn/me": { takes: never; gives: {
  id: string;
  email: string;
  name: string;
} };
  "GET /api/learn/videos/{id}": { takes: never; gives: void };
  "GET /api/mail/campaigns": { takes: never; gives: Page<Campaign> };
  "GET /api/mail/letters": { takes: never; gives: ({
  kind: string;
  language: string;
  subject: string;
  body: string;
  theirs: boolean;
  names: string[];
})[] };
  "GET /api/mail/lists": { takes: never; gives: Page<MailList> };
  "GET /api/mail/lists/{id}/subscribers": { takes: never; gives: Page<Subscriber> };
  "GET /api/media": { takes: never; gives: Page<Media> };
  "GET /api/orders": { takes: never; gives: Page<Order> };
  "GET /api/orders/{id}": { takes: never; gives: {
  order: Order;
  lines: Bought[];
} };
  "GET /api/overview": { takes: never; gives: {
  days: number;
  totals: Totals;
  written: Count[];
  arrived: Count[];
  visitors: Day[];
  mail: Mail;
} };
  "GET /api/pages/issues": { takes: never; gives: Page<Issue> };
  "GET /api/people": { takes: never; gives: Page<Person> };
  "GET /api/plugins": { takes: never; gives: ({
  key: string;
  configured: boolean;
  enabled: boolean;
  settings: unknown;
  holds: string[];
  working?: boolean | null;
  note?: string | null;
})[] };
  "GET /api/portable/export": { takes: never; gives: {
  version: number;
  languages?: BundledLanguage[];
  terms?: BundledTerm[];
  posts?: BundledPost[];
} };
  "GET /api/posts": { takes: never; gives: Page<Post> };
  "GET /api/posts/counts": { takes: never; gives: {
  draft: number;
  scheduled: number;
  published: number;
  archived: number;
} };
  "GET /api/posts/{id}": { takes: never; gives: {
  post: Post;
  term_ids: string[];
  translations: Translation[];
} };
  "GET /api/posts/{id}/issues": { takes: never; gives: ({
  post_id: string;
  title: string;
  kind: string;
  weight: string;
  detail: unknown;
  written_at: string;
})[] };
  "GET /api/products": { takes: never; gives: Page<Product> };
  "GET /api/reports": { takes: never; gives: Page<Report> };
  "GET /api/roles": { takes: never; gives: ({
  id: string;
  key: string;
  name: string;
  grants: string[];
  built_in: boolean;
})[] };
  "GET /api/setup": { takes: never; gives: {
  needed: boolean;
} };
  "GET /api/site": { takes: never; gives: {
  name: string;
  storage_used_bytes: number;
  storage_limit_bytes?: number | null;
} };
  "GET /api/sites/orders/{id}": { takes: never; gives: {
  id: string;
  number: number;
  state: OrderState;
  email: string;
  total: Money;
  created_at: string;
} };
  "GET /api/sites/products": { takes: never; gives: Page<Product> };
  "GET /api/students": { takes: never; gives: Page<Student> };
  "GET /api/students/{id}/enrolments": { takes: never; gives: ({
  id: string;
  course_id: string;
  course: string;
  ends_at?: string | null;
  state: string;
  created_at: string;
})[] };
  "GET /api/terms": { takes: never; gives: Page<Term> };
  "GET /api/trash": { takes: never; gives: Page<Thrown> };
  "GET /api/videos": { takes: never; gives: Page<Video> };
  "GET /api/videos/{id}": { takes: never; gives: {
  id: string;
  media_id?: string | null;
  title: string;
  state: VideoState;
  seconds?: number | null;
  plays: unknown;
  note?: string | null;
  created_at: string;
} };
  "GET /llms.txt": { takes: never; gives: void };
  "GET /uploads/{id}": { takes: never; gives: void };
  "PATCH /api/auth/password": { takes: {
  current: Secret_String;
  next: Secret_String;
}; gives: void };
  "PATCH /api/boards/{id}": { takes: {
  name: Title;
}; gives: {
  id: string;
  name: string;
  created_at: string;
} };
  "PATCH /api/cards/{id}": { takes: {
  title?: null | Title;
  detail?: string | null;
  stage_id?: string | null;
  position?: number | null;
  owner_id?: string | null;
}; gives: {
  id: string;
  stage_id: string;
  title: string;
  detail?: string | null;
  owner_id?: string | null;
  value?: null | Money;
  position: number;
  created_at: string;
} };
  "PATCH /api/courses/{id}": { takes: {
  title?: null | Title;
  summary?: string | null;
  state?: string | null;
}; gives: {
  id: string;
  slug: string;
  title: string;
  summary?: string | null;
  state: CourseState;
  created_at: string;
} };
  "PATCH /api/enrolments/{id}": { takes: {
  days?: number | null;
  forever?: boolean | null;
}; gives: {
  id: string;
  course_id: string;
  course: string;
  ends_at?: string | null;
  state: string;
  created_at: string;
} };
  "PATCH /api/flows/{id}": { takes: {
  name?: null | Title;
  active?: boolean | null;
  steps?: NewStep[] | null;
}; gives: {
  flow: Flow;
  steps: FlowStep[];
} };
  "PATCH /api/forms/{id}": { takes: {
  name?: null | Title;
  fields?: FormField[] | null;
  active?: boolean | null;
  retention_days?: number | null;
}; gives: {
  id: string;
  slug: string;
  name: string;
  fields: unknown;
  active: boolean;
  retention_days: number;
  submissions: number;
  unseen: number;
  created_at: string;
} };
  "PATCH /api/languages/{code}": { takes: {
  name?: null | Title;
  is_default?: boolean | null;
}; gives: {
  id: string;
  code: string;
  name: string;
  is_default: boolean;
  posts: number;
} };
  "PATCH /api/lessons/{id}": { takes: {
  title?: null | Title;
  position?: number | null;
  body?: string | null;
  video_id?: string | null;
}; gives: {
  id: string;
  title: string;
  position: number;
  video_id?: string | null;
  done: boolean;
} };
  "PATCH /api/modules/{id}": { takes: {
  title?: null | Title;
  position?: number | null;
}; gives: {
  id: string;
  title: string;
  position: number;
  lessons: Lesson[];
} };
  "PATCH /api/people/{id}": { takes: {
  name?: null | Title;
  email?: null | Email;
  role_id?: string | null;
  suspended?: boolean | null;
}; gives: {
  id: string;
  email: string;
  name: string;
  role: string;
  state: PersonState;
  email_proved: boolean;
  created_at: string;
} };
  "PATCH /api/posts/{id}": { takes: {
  title?: null | Title;
  slug?: null | Slug;
  excerpt?: string | null;
  body?: string | null;
  fields?: Record<string, unknown> | null;
  type?: string | null;
  state?: null | State;
  publish_at?: string | null;
  cover_media_id?: string | null;
  seo_title?: string | null;
  seo_description?: string | null;
  canonical?: string | null;
}; gives: {
  id: string;
  kind: Kind;
  state: State;
  language: string;
  slug: string;
  title: string;
  excerpt?: string | null;
  body: string;
  fields: unknown;
  type?: string | null;
  author_id?: string | null;
  cover_media_id?: string | null;
  seo_title?: string | null;
  seo_description?: string | null;
  canonical?: string | null;
  translation_of?: string | null;
  published_at?: string | null;
  created_at: string;
} };
  "PATCH /api/products/{id}": { takes: {
  name?: null | Title;
  description?: string | null;
  price_minor?: number | null;
  stock?: number | null;
  low_stock_at?: number | null;
  active?: boolean | null;
}; gives: {
  id: string;
  slug: string;
  name: string;
  description?: string | null;
  price: Money;
  stock: number;
  active: boolean;
  created_at: string;
} };
  "PATCH /api/roles/{id}": { takes: {
  name?: null | Title;
  grants?: string[] | null;
}; gives: {
  id: string;
  key: string;
  name: string;
  grants: string[];
  built_in: boolean;
} };
  "PATCH /api/site": { takes: {
  name: Title;
}; gives: {
  name: string;
  storage_used_bytes: number;
  storage_limit_bytes?: number | null;
} };
  "PATCH /api/students/{id}": { takes: {
  name?: null | Title;
  suspended?: boolean | null;
}; gives: {
  id: string;
  email: string;
  name: string;
  state: StudentState;
  last_seen_at?: string | null;
  courses: number;
  created_at: string;
} };
  "PATCH /api/terms/{id}": { takes: {
  name?: null | Title;
  slug?: null | Slug;
  description?: string | null;
  parent_id?: string | null;
}; gives: {
  id: string;
  kind: TermKind;
  language: string;
  slug: string;
  name: string;
  description?: string | null;
  parent_id?: string | null;
  created_at: string;
} };
  "POST /api/assistant/handover": { takes: never; gives: {
  id: string;
  token: Shown;
  expires_at: string;
  grants: string[];
} };
  "POST /api/auth/oauth/{key}/callback": { takes: {
  code: string;
  state: string;
  redirect_uri: string;
  second_factor?: string | null;
}; gives: {
  token: Shown;
  expires_at: string;
  user: Me;
  redirect: string;
} };
  "POST /api/auth/oauth/{key}/start": { takes: {
  redirect?: string | null;
  redirect_uri: string;
}; gives: {
  url: string;
  state: string;
} };
  "POST /api/auth/password": { takes: {
  token: Secret_String;
  password: Secret_String;
}; gives: void };
  "POST /api/auth/reset": { takes: {
  email: Email;
}; gives: void };
  "POST /api/auth/second-factor": { takes: never; gives: {
  secret: Shown;
  uri: string;
} };
  "POST /api/auth/second-factor/confirm": { takes: {
  code: string;
}; gives: {
  codes: string[];
} };
  "POST /api/auth/session": { takes: {
  email: Email;
  password: Secret_String;
  code?: string | null;
}; gives: {
  token: Shown;
  expires_at: string;
  user: Me;
} };
  "POST /api/boards": { takes: {
  name: Title;
  stages?: Title[];
}; gives: {
  id: string;
  name: string;
  created_at: string;
} };
  "POST /api/boards/{id}/cards": { takes: {
  stage_id: string;
  title: Title;
  detail?: string | null;
  owner_id?: string | null;
  value_minor?: number | null;
  currency?: null | Currency;
}; gives: {
  id: string;
  stage_id: string;
  title: string;
  detail?: string | null;
  owner_id?: string | null;
  value?: null | Money;
  position: number;
  created_at: string;
} };
  "POST /api/cards/{id}/notes": { takes: {
  body: string;
}; gives: void };
  "POST /api/content-types": { takes: {
  plural?: null | Title;
  names?: unknown;
  key?: string | null;
  name: Title;
  fields: TypeField[];
}; gives: {
  id: string;
  key: string;
  name: string;
  plural?: string | null;
  names: unknown;
  fields: unknown;
  posts: number;
} };
  "POST /api/coupons": { takes: {
  code: string;
  kind: string;
  value: number;
  uses_allowed?: number | null;
  minimum_minor?: number | null;
  per_shopper?: number | null;
  currency?: null | Currency;
  expires_at?: string | null;
}; gives: {
  id: string;
  code: string;
  kind: string;
  value: number;
  uses_allowed?: number | null;
  minimum_minor: number;
  per_shopper?: number | null;
  currency: Currency;
  used: number;
  expires_at?: string | null;
  created_at: string;
} };
  "POST /api/courses": { takes: {
  slug: Slug;
  title: Title;
  summary?: string | null;
}; gives: {
  id: string;
  slug: string;
  title: string;
  summary?: string | null;
  state: CourseState;
  created_at: string;
} };
  "POST /api/courses/{id}/modules": { takes: {
  title: Title;
  position?: number | null;
}; gives: {
  id: string;
  title: string;
  position: number;
  lessons: Lesson[];
} };
  "POST /api/courses/{id}/students": { takes: {
  days?: number | null;
  email: Email;
  name: Title;
}; gives: {
  student_id: string;
  token: Shown;
} };
  "POST /api/design/previews": { takes: never; gives: {
  id: string;
  branch: string;
  state: PublishState;
  seconds?: number | null;
  log?: string | null;
  created_at: string;
} };
  "POST /api/design/publishes": { takes: never; gives: {
  id: string;
  branch: string;
  state: PublishState;
  seconds?: number | null;
  log?: string | null;
  created_at: string;
} };
  "POST /api/design/publishes/{id}/cancel": { takes: never; gives: {
  id: string;
  branch: string;
  state: PublishState;
  seconds?: number | null;
  log?: string | null;
  created_at: string;
} };
  "POST /api/flows": { takes: {
  name: Title;
  trigger: string;
  steps?: NewStep[];
}; gives: {
  id: string;
  name: string;
  trigger: string;
  active: boolean;
  created_at: string;
} };
  "POST /api/flows/credentials": { takes: {
  name: Title;
  secret: Secret_String;
}; gives: void };
  "POST /api/forms": { takes: {
  slug: Slug;
  name: Title;
  fields?: FormField[];
  retention_days?: number | null;
}; gives: {
  id: string;
  slug: string;
  name: string;
  fields: unknown;
  active: boolean;
  retention_days: number;
  submissions: number;
  unseen: number;
  created_at: string;
} };
  "POST /api/forms/{id}/seen": { takes: never; gives: {
  seen: number;
} };
  "POST /api/languages": { takes: {
  code: string;
  name: Title;
  is_default?: boolean;
}; gives: {
  id: string;
  code: string;
  name: string;
  is_default: boolean;
  posts: number;
} };
  "POST /api/learn/lessons/{id}/done": { takes: never; gives: void };
  "POST /api/learn/session": { takes: {
  email: Email;
  password: Secret_String;
}; gives: void };
  "POST /api/mail/campaigns": { takes: {
  list_id: string;
  subject: Title;
  body: string;
}; gives: {
  id: string;
  list_id: string;
  subject: string;
  state: CampaignState;
  sent_count: number;
  created_at: string;
} };
  "POST /api/mail/campaigns/{id}/send": { takes: never; gives: {
  id: string;
  list_id: string;
  subject: string;
  state: CampaignState;
  sent_count: number;
  created_at: string;
} };
  "POST /api/mail/events": { takes: {
  provider_ref: string;
  kind: string;
  detail?: string | null;
}; gives: void };
  "POST /api/mail/lists": { takes: {
  name: Title;
}; gives: {
  id: string;
  name: string;
  created_at: string;
} };
  "POST /api/mail/lists/{id}/subscribers": { takes: {
  email: Email;
  name?: string | null;
}; gives: {
  id: string;
  email: string;
  name?: string | null;
  state: SubscriberState;
  created_at: string;
} };
  "POST /api/media": { takes: never; gives: {
  id: string;
  original_name: string;
  mime: string;
  bytes: number;
  created_at: string;
} };
  "POST /api/modules/{id}/lessons": { takes: {
  title: Title;
  position?: number | null;
  body?: string | null;
  video_id?: string | null;
}; gives: {
  id: string;
  title: string;
  position: number;
  video_id?: string | null;
  done: boolean;
} };
  "POST /api/orders/{id}/fulfilled": { takes: never; gives: {
  id: string;
  number: number;
  state: OrderState;
  email: string;
  total: Money;
  created_at: string;
} };
  "POST /api/orders/{id}/paid": { takes: never; gives: {
  id: string;
  number: number;
  state: OrderState;
  email: string;
  total: Money;
  created_at: string;
} };
  "POST /api/orders/{id}/refund": { takes: never; gives: {
  id: string;
  number: number;
  state: OrderState;
  email: string;
  total: Money;
  created_at: string;
} };
  "POST /api/people": { takes: {
  email: Email;
  name: Title;
  role_id: string;
}; gives: {
  id: string;
  expires_at: string;
} };
  "POST /api/people/erase": { takes: {
  email: Email;
}; gives: void };
  "POST /api/people/export": { takes: {
  email: Email;
}; gives: {
  email: string;
  found: unknown;
} };
  "POST /api/plugins/{key}/check": { takes: never; gives: {
  key: string;
  configured: boolean;
  enabled: boolean;
  settings: unknown;
  holds: string[];
  working?: boolean | null;
  note?: string | null;
} };
  "POST /api/portable/import": { takes: {
  version: number;
  languages?: BundledLanguage[];
  terms?: BundledTerm[];
  posts?: BundledPost[];
}; gives: {
  languages: number;
  terms: number;
  posts: number;
  left_alone: number;
} };
  "POST /api/posts": { takes: {
  language: string;
  title: Title;
  slug?: null | Slug;
  kind?: null | Kind;
  excerpt?: string | null;
  body?: string | null;
  fields?: Record<string, unknown> | null;
  type?: string | null;
  cover_media_id?: string | null;
  seo_title?: string | null;
  seo_description?: string | null;
  canonical?: string | null;
  translation_of?: string | null;
}; gives: {
  id: string;
  kind: Kind;
  state: State;
  language: string;
  slug: string;
  title: string;
  excerpt?: string | null;
  body: string;
  fields: unknown;
  type?: string | null;
  author_id?: string | null;
  cover_media_id?: string | null;
  seo_title?: string | null;
  seo_description?: string | null;
  canonical?: string | null;
  translation_of?: string | null;
  published_at?: string | null;
  created_at: string;
} };
  "POST /api/posts/actions": { takes: {
  act: string;
  ids: string[];
}; gives: {
  acted_on: number;
  left_alone: string[];
} };
  "POST /api/products": { takes: {
  slug: Slug;
  name: Title;
  description?: string | null;
  price_minor: number;
  currency: Currency;
  stock?: number | null;
  low_stock_at?: number | null;
}; gives: {
  id: string;
  slug: string;
  name: string;
  description?: string | null;
  price: Money;
  stock: number;
  active: boolean;
  created_at: string;
} };
  "POST /api/reports": { takes: {
  kind?: string;
  environment?: unknown;
  media_id?: string | null;
  screen?: string | null;
  body: string;
}; gives: {
  id: string;
  kind: ReportKind;
  environment: unknown;
  media_id?: string | null;
  screen?: string | null;
  body: string;
  state: ReportState;
  answer?: string | null;
  answered_at?: string | null;
  created_at: string;
} };
  "POST /api/roles": { takes: {
  key: string;
  name: Title;
  grants: string[];
}; gives: {
  id: string;
  key: string;
  name: string;
  grants: string[];
  built_in: boolean;
} };
  "POST /api/setup": { takes: {
  email: Email;
  name: Title;
  password: Secret_String;
}; gives: {
  needed: boolean;
} };
  "POST /api/sites/beacon": { takes: {
  path: string;
  lcp?: number | null;
  inp?: number | null;
  cls?: number | null;
  ttfb?: number | null;
}; gives: void };
  "POST /api/sites/checkout": { takes: {
  email: Email;
  items: Wanted[];
  coupon?: string | null;
  idempotency_key: string;
}; gives: {
  order: Order;
  pay_at?: string | null;
} };
  "POST /api/sites/forms/{slug}/submissions": { takes: {
  answers: Record<string, unknown>;
}; gives: {
  id: string;
} };
  "POST /api/sites/payments/callback": { takes: never; gives: void };
  "POST /api/sites/unsubscribe": { takes: {
  token: Secret_String;
}; gives: void };
  "POST /api/sites/videos/callback": { takes: {
  reference: string;
  ok: boolean;
  seconds?: number | null;
  plays?: unknown;
  note?: string | null;
}; gives: void };
  "POST /api/terms": { takes: {
  kind: TermKind;
  language: string;
  name: Title;
  slug?: null | Slug;
  description?: string | null;
  parent_id?: string | null;
}; gives: {
  id: string;
  kind: TermKind;
  language: string;
  slug: string;
  name: string;
  description?: string | null;
  parent_id?: string | null;
  created_at: string;
} };
  "POST /api/trash/{kind}/{id}": { takes: never; gives: {
  kind: string;
  id: string;
  name: string;
  thrown_at: string;
  goes_at: string;
} };
  "POST /api/videos": { takes: {
  media_id: string;
  title: Title;
}; gives: {
  id: string;
  media_id?: string | null;
  title: string;
  state: VideoState;
  seconds?: number | null;
  plays: unknown;
  note?: string | null;
  created_at: string;
} };
  "POST /mcp": { takes: {
  method: string;
  params?: unknown;
}; gives: void };
  "PUT /api/auth/oauth/{key}": { takes: {
  label: string;
  client_id: string;
  client_secret: Secret_String;
  authorize_url: string;
  token_url: string;
  profile_url: string;
  scope?: string;
  enabled?: boolean;
}; gives: {
  key: string;
  label: string;
} };
  "PUT /api/content-types/{key}": { takes: {
  plural?: null | Title;
  names?: unknown;
  key?: string | null;
  name: Title;
  fields: TypeField[];
}; gives: {
  id: string;
  key: string;
  name: string;
  plural?: string | null;
  names: unknown;
  fields: unknown;
  posts: number;
} };
  "PUT /api/design/files": { takes: {
  path: string;
  body: string;
  branch?: string | null;
}; gives: {
  path: string;
  branch: string;
  updated_at: string;
} };
  "PUT /api/mail/letters/{kind}": { takes: {
  language: string;
  subject: string;
  body: string;
}; gives: {
  kind: string;
  language: string;
  subject: string;
  body: string;
  theirs: boolean;
  names: string[];
} };
  "PUT /api/plugins/{key}": { takes: {
  settings: unknown;
  enabled?: boolean;
}; gives: {
  key: string;
  configured: boolean;
  enabled: boolean;
  settings: unknown;
  holds: string[];
  working?: boolean | null;
  note?: string | null;
} };
  "PUT /api/posts/{id}/terms": { takes: {
  term_ids: string[];
}; gives: ({
  id: string;
  kind: TermKind;
  language: string;
  slug: string;
  name: string;
  description?: string | null;
  parent_id?: string | null;
  created_at: string;
})[] };
}
