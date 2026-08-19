# Mavi Open Source feature matrix

This is the source-of-truth checklist for the clean implementation. A row is
complete only when its API, storage model, authorization, audit behavior,
acceptance tests and panel contract are present. The old `server/` behavior is
reference material, not a design constraint.

Status: `[ ]` planned, `[-]` in progress, `[x]` complete.

## Foundation

- [x] Typed IDs, `SiteId`, `SiteContext`, caller types and error codes.
- [x] Grant/capability model, pagination, money and adapter ports.
- [x] Provider credential IDs, metadata-only DTOs and the `Seals` adapter port.
- [x] AES-GCM keyring adapter authenticates ciphertext to `SiteContext` and supports key rotation reads.
- [x] Site catalog, site-scoped transaction and first migration.
- [x] Canonical endpoint declaration with auth, permission, scope and mutation validation.
- [x] Embedded Cedar authorizer with principal/resource/site-scope tests.
- [x] OpenAPI 3.1 document generation from canonical endpoint declarations.
- [x] Fixed-site runtime composition for self-host.
- [x] Request admission middleware creates and validates `SiteContext`.
- [x] Site-scoped, token-owned write fences protect HTTP and MCP mutations during controlled relocation.
- [x] Cloud shard runtime resolves a site without a per-site router/process.
- [x] Versioned runtime manifest exposes release/API fingerprint, storage schema,
  runtime mode and cursor-only pagination policy to operator and panel clients.
- [x] OpenAPI, TypeScript/Rust client and MCP tool generation from the same contract.
  - [x] HTTP composition root combines domain endpoint declarations into one validated catalog.
  - [x] OpenAPI snapshot, typed TypeScript/Rust client artifacts and MCP tool generation.
  - [x] JSON and query inputs reject unknown top-level fields with a typed validation error; nested open maps remain explicit.
- [-] RLS/DB guard, composite foreign keys and site-aware unique constraints across every domain table.
  - [x] Site catalog, content, identity, audit and settings tables enforce RLS and site-aware keys.
  - [x] Design changes, files, builds and artifacts enforce RLS, composite foreign keys and site-aware keys.
  - [x] Forms and submissions enforce RLS, composite foreign keys and site-aware keys.
  - [x] Mail templates, lists, readers, deliveries and delivery attempts enforce RLS, composite foreign keys and site-aware keys.
  - [x] Shop products, coupons, order counters, orders, lines, holds and coupon uses enforce RLS, composite foreign keys and site-aware keys.
  - [x] Courses, modules, lessons, students, sessions, enrollments and progress enforce RLS, composite foreign keys and site-aware keys.
  - [x] Jobs, automation flows and run history enforce RLS, composite foreign keys and site-aware idempotency/order keys.
  - [x] Boards, lists, cards, comments, immutable activity and analytics tables enforce RLS and site-scoped keys.
  - [x] Portable imports write through the target site scope and preserve composite foreign-key boundaries.
  - [ ] Remaining domain tables and a single reusable DB guard for every repository.
- [-] Migration and schema integration tests against PostgreSQL.
  - [x] Storage, identity and settings migrations run against a non-superuser PostgreSQL role.
  - [ ] Every future domain adds its migration and isolation suite before completion.

## Setup, identity and access

- [-] First-run setup and site bootstrap.
  - [x] Public setup is site-scoped and serializes concurrent initialization.
  - [x] Owner role, full initial grants and setup audit receipt are transactional.
- [-] People, account sessions, password recovery and API keys.
  - [x] Site-scoped people list/create/status endpoints use typed DTOs and cursors.
  - [x] Passwords are Argon2id digests; setup and person DTO debug output redacts secrets.
  - [x] Session and API-key authentication/revocation are site-scoped and audited.
  - [x] Provider credentials are site-scoped, sealed at rest, optimistic-versioned and never returned as values.
  - [x] Password recovery uses hashed one-time tokens, atomic redemption, session revocation, generic responses and a transactional mail outbox.
  - [x] Email verification uses site-scoped hashed one-time tokens, generic request responses, transactional mail, an explicit login gate and atomic redemption.
  - [x] Reset and verification mail bodies are AES-GCM sealed with site-bound associated data; the outbox/API expose only a redaction marker and workers unseal only with the keyring capability.
- [-] Roles, Cedar-backed grants, assistant delegation and revocation.
  - [x] Site-scoped role list/create/grant replacement endpoints are canonicalized.
  - [x] Cedar authorizes HTTP resources and role/person grant delegation cannot escalate.
  - [-] Role deletion, ownership invariants and full assistant lifecycle UI.
    - [x] Protected owner role, assigned-role deletion conflict, audited unassigned-role deletion and canonical `roles.delete` contract are implemented in this slice; assistant lifecycle UI remains open.
    - [x] Account-only API-key metadata listing, one-time token creation response, assistant self-revocation guard and revoked-key audit lifecycle are implemented in this slice.
- [x] Student identity isolated from panel accounts.
  - [x] Invitation tokens are single-use hashes; activation creates a separate expiring student session.
  - [x] Student sessions carry no panel grants and are rejected by account/operator endpoints.
- [-] Rate limits, request audit identity and security events.
  - [x] Password-reset and email-verification requests have a site-scoped five-per-hour subject throttle and generic rate-limit audit outcomes.
  - [x] Failed logins and unverified-login blocks write anonymous-safe security audit events without storing raw email addresses.
  - [x] Authentication edge actions use bounded site+action IP/device windows, trusted-proxy parsing, `Retry-After`, hashed source signals and one audit receipt per source/action window.
  - [x] Account-security audit actions use a canonical taxonomy for session, recovery, verification, API-key, subject-throttle and edge-throttle outcomes.

## Site configuration and content

- [-] Site settings, URL, timezone and language configuration.
  - [x] Site settings and language list/create/update/delete APIs use typed DTOs, Cedar grants and audit receipts.
  - [x] Site language defaults are serialized per site and cannot be removed without a replacement.
  - [x] Canonical site URLs are normalized, query-free absolute HTTP(S) values with explicit clear/set PATCH semantics.
  - [x] Public content resolves exact language, regional base language and configured default language in order.
- [x] Content types and validated custom fields.
  - [x] Site-scoped content type declarations use `PUT` upsert and opaque cursor listing.
  - [x] Declared custom fields validate required values, types, choices and unknown keys.
  - [x] Removing a declaration preserves existing content and its stored fields.
- [-] Posts/pages, drafts, revisions, slugs, scheduling and public reads.
  - [x] Content create/update/public lifecycle and immutable revision history API.
  - [x] Slug history preserves old published public paths after a slug change.
  - [-] Scheduled publishing worker/queue and rollback/restore UX.
    - [x] Scheduling atomically enqueues a site-scoped, idempotent `content.publish_scheduled` job.
    - [x] Site-scoped content worker claims leases, publishes due content, defers early jobs and treats stale schedules as safe no-ops.
    - [x] Revision restore creates a new audited draft revision; publishing the restored snapshot remains an explicit permissioned action.
- [-] Taxonomy terms, trees, assignment and filtered listing.
  - [x] Site-scoped category/tag terms with language-aware slugs and opaque cursor listing.
  - [x] Category parent validation, recursive cycle protection and atomic content assignments.
  - [x] Content-to-term reads, replacement and term membership listing use canonical API contracts.
  - [x] Public taxonomy archive URLs resolve exact/regional/default language terms and return only published content through opaque cursors.
- [x] Media metadata, uploads, image variants, file storage and cleanup.
  - [x] Site-scoped file metadata, byte-sniffed allowlist, SHA-256 receipt and opaque cursor listing.
  - [x] Raw binary upload contract, local atomic file adapter and in-memory test adapter.
  - [x] Explicit private/public visibility, authenticated downloads and public downloads with integrity verification.
  - [x] RLS/composite keys, Cedar media grants, upload/trash audit receipts and durable cleanup tasks.
  - [x] Private shard relocation exports live metadata plus verified bytes and retries through the site-scoped `FileStore`.
- [x] Image variants and orphan cleanup worker.
  - [x] Permanent media deletion creates an idempotent site-scoped cleanup job; the shared worker removes bytes through `FileStore`, records an immutable completion audit and reopens dead jobs for later retry.
  - [x] Shared worker enumerates one site namespace, preserves live and pending-cleanup keys, removes only generated media keys and records the removed count.
  - [x] Image variants use a site-scoped `media_variants` table with composite source foreign keys and three deterministic presets (`thumbnail`, `medium`, `large`).
  - [x] Variant generation is bounded, blocking-work isolated, JPEG-normalized, idempotent and converges imported/legacy images through worker discovery.
  - [x] Authenticated and public variant APIs verify source visibility, byte length and SHA-256; variant metadata never exposes storage keys.
  - [x] Permanent deletion queues original and derived keys together; restore retains derived data and orphan cleanup preserves every live/pending key.

## Operations and publishing

- [-] Audit receipts for every mutation and auditable actor attribution.
  - [x] Immutable site-scoped receipts with typed actor/resource fields and atomic writes.
  - [x] Cursor-filtered audit list/read API with Cedar grants and generated contracts.
  - [ ] Export/download retention policy and security-event coverage for future domains.
- [-] Trash, restore and permanent deletion policy.
  - [x] Shared cursor list, typed content/file/term restore and permanent-delete API.
  - [x] Media trash retains bytes; permanent deletion queues and confirms adapter cleanup.
  - [ ] Forms, shop, courses, boards and flow-specific trash kinds plus scheduled retention worker.
- [-] Design files, preview builds, publish, rollback and public serving.
  - [x] Site-scoped design changes copy the current published source and expose only typed source-file APIs.
  - [x] Opaque keyset cursors are used for changes, files and builds; `page`/`offset` are not public inputs.
  - [x] Static self-host builds publish only `public/` files and require `public/index.html`; `src/` is never served.
  - [x] Ready builds are immutable; publish and rollback atomically switch the live build pointer and write audit receipts.
  - [x] Preview/live serving is routed through site-scoped `FileStore` metadata and Cedar protects management APIs.
  - [x] Private shard relocation exports design history, source files, build metadata and verified artifact bytes; publish pointers are restored last.
  - [ ] Sandboxed cloud compiler adapter, panel design screens and asynchronous build worker.
- [-] Forms, submissions, exports, spam controls and retention policy.
  - [x] Form declarations validate bounded fields, types, choices and site-local active slugs.
  - [x] Public submissions validate required/typed/known answers and return a receipt without exposing management metadata.
  - [x] Submission inbox uses `after`/`limit` cursors, unread filtering, mark-read and audited deletion.
  - [x] Forms/submissions use composite keys, RLS, Cedar grants and mutation audit receipts.
  - [x] Per-form `kept_days` is enforced by an idempotent site-scoped `forms.retention` job; expired answers are redacted behind a tombstone and the system audit receipt commits atomically, with worker and cross-site tests.
  - [x] Versioned `mavi.forms.submissions` JSON export returns only active rows through a bounded opaque cursor, includes the form declaration, and records an audit receipt in the same transaction.
  - [x] Public submissions enter a site+action edge window keyed by privacy-preserving peer/device signals; limit responses expose `Retry-After` and write one site-scoped `forms.security.edge_rate_limited` audit receipt per source window.
- [-] Mail templates, delivery queue, retries and provider adapters.
  - [x] Strict site-scoped templates render bounded `{{variable}}` placeholders and expose preview without sending.
  - [x] Mailing lists/readers use normalized addresses, hashed unsubscribe tokens and explicit standing states.
  - [x] Transactional and campaign requests enqueue provider-neutral outbox rows; public/API requests never call a provider.
  - [x] Workers claim leases, record attempts, mark sent/retry/dead and support idempotency keys.
  - [x] Security-sensitive transactional bodies are sealed at rest, fail closed without a sealer, and relocate as cancelled redacted records without ciphertext.
  - [x] Domain code uses the shared `Mailer` port and returns provider receipts without coupling to SMTP/cloud SDKs.
  - [x] The worker receives a host-owned `Mailer`, claims one delivery before the provider call, passes the durable delivery/attempt/idempotency contract, and records success or retry in a separate site-scoped transaction; mail and maintenance jobs are scheduled fairly so either queue cannot starve.
  - [x] Campaigns require a site canonical URL, inject a reserved `unsubscribe_url`
    template variable, seal the bearer link separately, and consume one-time
    hashed campaign tokens without exposing the URL in delivery DTOs.
  - [x] The self-host composition root provides a fail-closed HTTPS webhook
    Mailer adapter with typed purpose/attempt/idempotency metadata and provider
    receipt validation; SMTP/cloud SDKs remain behind that host boundary.
  - [x] Normalized provider-event webhook, site-scoped idempotency, permanent
    bounce/complaint suppression and queued campaign cancellation.
  - [x] Site settings carry a validated sender identity, each queued delivery
    snapshots it, and the host Mailer enforces its deployment sender-domain
    allowlist before emitting a provider `From` header.
  - [x] Provider HTTP 429 responses preserve a bounded `Retry-After` hint and
    the worker schedules that delay without exceeding the delivery attempt cap.
  - [ ] Vendor-specific signature/payload adapters remain host/cloud
    deliverability work.

## Commerce and learning

- [-] Products, variants, prices, stock and site-local order numbering.
  - [x] Site-scoped product catalog uses Money value objects, immutable currency, soft deletion and cursor-only management/public lists.
  - [x] Checkout sorts product locks, snapshots names/prices, holds available stock and uses email-scoped idempotency keys.
  - [x] Site-local order counters and explicit waiting/paid/sent/called-off/given-back state transitions are transactional and audited.
  - [ ] Product variants and digital/physical fulfillment policy.
- [-] Checkout, payment adapter, refunds, discounts and order audit.
  - [x] Coupon percentage/amount rules, expiry/max-use locking and coupon-use audit boundaries.
  - [x] Payment remains a shared adapter boundary; HTTP only records an external receipt and never calls a provider inline.
  - [ ] Concrete payment providers, refund policy and payment webhook worker.
- [-] Courses, modules, lessons, video/file access and student enrollment.
  - [x] Course lifecycle is monotonic (`draft` → `open` → `closed`); ordered module/lesson writes are atomic and closed courses reject content changes.
  - [x] Student invitations, activation/login, enrollment and self-only learning routes use typed DTOs and opaque cursors.
  - [x] Lesson media is served as protected bytes only after enrollment, standing and open-course checks.
  - [x] Course tables use composite site keys, foreign keys and RLS; mutations emit audit receipts.
- [-] Expiring access, progress, completion and instructor permissions.
  - [x] Student sessions expire, stopped students lose access, and lesson completion is idempotent while retaining progress after unenrollment.
  - [x] Course-specific instructor assignments use a site-scoped `course_instructors` table, allow only `view`/`write`/`delete`, and are evaluated by Cedar as resource grants inside the same scoped transaction as course reads and mutations.

## Automation and collaboration

- [-] Flows, triggers, steps, retries, idempotency and dead-letter handling.
  - [x] Flow definitions validate a bounded trigger/step vocabulary and step configuration before persistence.
  - [x] Event fan-out is transactional, source-key idempotent and connected to registered durable jobs.
  - [x] Runs snapshot their definition, record every step attempt and expose simulation/run history APIs.
  - [ ] Concrete mail, webhook and list-mutating executors plus event emission from every producer domain.
- [-] Jobs, site-scoped queue leases and worker execution.
  - [x] Registered job kinds, site-scoped queue rows, composite idempotency and cursor-only admin lists.
  - [x] `FOR UPDATE SKIP LOCKED` claims, per-claim fencing tokens, lease heartbeat, stale-worker protection, bounded backoff and dead-letter retry.
  - [x] Shared worker supervisor owns one replaceable site snapshot and exports process-local metrics for polls, claims, outcomes, lease loss and errors.
  - [x] Concrete cloud adapter wiring in the private operator release — vucodcom/mavi-operator#246 (`1fa6600`); shared supervisor, active-site snapshot reconciliation and process metrics are wired against public Mavi `0dfac94c`.
- [-] Boards, lists, cards, assignments, comments and activity history.
  - [x] Board/list/card APIs use integer positions, transactional reorder/move operations and opaque keyset cursors.
  - [x] Card assignees are checked against active site people; comments support author-only editing and soft deletion.
  - [x] Every board mutation emits an audit receipt and append-only activity row; activity cannot be updated or deleted.
  - [ ] Card labels, due dates, mentions, collaboration notifications and panel screens.
- [-] Analytics events, aggregation, retention and export.
  - [x] Public ingestion accepts only bounded event names, route paths and non-negative numeric values; no arbitrary properties or visitor identifiers.
  - [x] Raw event export and daily aggregates use opaque cursors; daily rollups are updated in the ingestion transaction.
  - [x] Raw and aggregate retention pruning is bounded, explicit and audit-recorded.
  - [x] Public analytics ingestion uses the shared privacy-preserving site/IP/device edge limiter and emits auditable `security.edge_rate_limited` outcomes with `Retry-After`.
  - [x] Site-configured raw/aggregate retention windows enqueue one idempotent UTC-day job; the shared worker prunes both tables with a system audit receipt.
  - [ ] Privacy documentation and panel charts.
- [-] Portable export/import with versioned manifests and validation.
  - [x] Explicit `mavi.portable` v2 bundles carry source-site provenance, canonical site URL, counts and a schema hash.
  - [x] Settings/languages, content types, taxonomy, content/revisions, slug history and assignments export/import with typed records.
  - [x] Import validates references before writes, supports validate-only/create-only/upsert strategies and applies atomically.
  - [x] Private shard relocation adds identity credentials, live media and design/build data without exposing credentials or binary data through public portable export.
  - [ ] Public media/design bytes, shop/courses/forms/boards/automation provider state and encrypted secret handling in later bundle versions.
- [-] MCP resources/tools generated from the canonical API with grant checks.
  - [x] Deterministic tool descriptors preserve authentication, scope and Cedar permission metadata.
  - [x] Stateless `2026-07-28` transport, `server/discover`, cursor-based `tools/list` and HTTP-routed `tools/call`.

## Runtime, panel and release

- [x] Shared HTTP router with request-level site scope.
- [x] Global `/healthz` liveness and database-backed `/readyz` readiness stay outside site admission middleware.
- [x] Outer HTTP telemetry assigns one request ID to every request, returns it as `x-request-id` and emits structured completion fields for JSON logs — productdevbook/mavi#221.
- [x] Clean self-host image builds only `server-next`, runs non-root, persists `/data/files` and exposes the fixed-site runtime.
- [ ] Self-host configuration and upgrade path.
- [-] Cloud/operator provisioning contract without direct database coupling.
  - [x] Mavi publishes a versioned runtime manifest for post-provision compatibility checks.
  - [ ] Operator consumes only a tagged Mavi release and verifies the manifest before activation.
- [ ] Panel generated client, stale-contract check and feature screens.
- [ ] Per-domain unit, repository, migration, isolation, application, API,
  HTTP, permission and audit tests.
  - [x] Identity unit, PostgreSQL scope/audit and negative delegation tests.
  - [x] Settings/languages unit, PostgreSQL scope/audit and HTTP cursor/default tests.
  - [x] Content type unit, PostgreSQL scope/audit/field validation and HTTP tests.
  - [x] Content lifecycle revision/slug/public HTTP and PostgreSQL tests.
  - [x] Taxonomy tree, assignment, RLS and HTTP permission/cursor tests.
  - [x] Audit receipt PostgreSQL isolation/cursor tests and audit/trash HTTP acceptance tests.
  - [x] Trash restore/permanent-delete PostgreSQL tests, including media cleanup receipts.
  - [x] Design source/build PostgreSQL isolation, immutable artifact, publish/rollback and HTTP serving tests.
  - [x] Forms declaration/submission/export PostgreSQL RLS isolation and HTTP validation/cursor/permission tests.
  - [x] Mail template/list/outbox PostgreSQL RLS state-machine tests and HTTP contract coverage.
  - [x] Shop catalog/checkout/stock/order PostgreSQL isolation tests and HTTP permission/contract coverage.
  - [x] Courses authoring/order/student-session/enrollment/progress/media PostgreSQL isolation tests and HTTP permission/contract coverage.
  - [x] Course instructor assignment PostgreSQL/RLS and Cedar resource-grant HTTP acceptance coverage.
  - [x] Jobs lease/idempotency/dead-letter PostgreSQL isolation and automation flow snapshot/event/run HTTP coverage.
  - [x] Boards PostgreSQL scope/order/activity tests and boards HTTP cursor/permission acceptance coverage.
  - [x] Analytics PostgreSQL aggregate/retention/isolation tests and analytics HTTP ingest/export coverage.
  - [x] Portable cross-site PostgreSQL export/import/conflict tests, private identity/media/design relocation and HTTP contract acceptance coverage.
  - [x] Identity HTTP integration covers setup, login, cursor, 401/403 and Cedar behavior.
  - [x] Identity edge throttling covers trusted-proxy behavior, 429/`Retry-After`, source redaction and one audit receipt per window.
  - [x] Generated OpenAPI/TypeScript/Rust/MCP artifacts have stale-contract tests.
  - [x] Credential sealing unit, PostgreSQL scope/audit/version and metadata-only HTTP acceptance tests.
  - [x] Runtime manifest HTTP contract and cursor-only compatibility assertions.
  - [ ] Remaining domain HTTP suites.
- [ ] Release smoke test, migration rollback policy, backups and upgrade docs.
- [ ] Operator consumes only a tagged Mavi release/API contract.
