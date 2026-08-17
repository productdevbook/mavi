# Mavi Open Source feature matrix

This is the source-of-truth checklist for the clean implementation. A row is
complete only when its API, storage model, authorization, audit behavior,
acceptance tests and panel contract are present. The old `server/` behavior is
reference material, not a design constraint.

Status: `[ ]` planned, `[-]` in progress, `[x]` complete.

## Foundation

- [x] Typed IDs, `SiteId`, `SiteContext`, caller types and error codes.
- [x] Grant/capability model, pagination, money and adapter ports.
- [x] Site catalog, site-scoped transaction and first migration.
- [x] Canonical endpoint declaration with auth, permission, scope and mutation validation.
- [x] Embedded Cedar authorizer with principal/resource/site-scope tests.
- [x] OpenAPI 3.1 document generation from canonical endpoint declarations.
- [x] Fixed-site runtime composition for self-host.
- [x] Request admission middleware creates and validates `SiteContext`.
- [ ] Cloud shard runtime resolves a site without a per-site router/process.
- [x] OpenAPI, TypeScript/Rust client and MCP tool generation from the same contract.
  - [x] HTTP composition root combines domain endpoint declarations into one validated catalog.
  - [x] OpenAPI snapshot, typed TypeScript/Rust client artifacts and MCP tool generation.
- [-] RLS/DB guard, composite foreign keys and site-aware unique constraints across every domain table.
  - [x] Site catalog, content, identity, audit and settings tables enforce RLS and site-aware keys.
  - [x] Design changes, files, builds and artifacts enforce RLS, composite foreign keys and site-aware keys.
  - [-] Forms and submissions enforce RLS, composite foreign keys and site-aware keys.
  - [x] Mail templates, lists, readers, deliveries and delivery attempts enforce RLS, composite foreign keys and site-aware keys.
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
  - [ ] Password recovery, email verification and account security events.
- [-] Roles, Cedar-backed grants, assistant delegation and revocation.
  - [x] Site-scoped role list/create/grant replacement endpoints are canonicalized.
  - [x] Cedar authorizes HTTP resources and role/person grant delegation cannot escalate.
  - [ ] Role deletion, ownership invariants and full assistant lifecycle UI.
- [ ] Student identity isolated from panel accounts.
- [ ] Rate limits, request audit identity and security events.

## Site configuration and content

- [-] Site settings, URL, timezone and language configuration.
  - [x] Site settings and language list/create/update/delete APIs use typed DTOs, Cedar grants and audit receipts.
  - [x] Site language defaults are serialized per site and cannot be removed without a replacement.
  - [ ] Canonical site URL and locale fallback policy.
- [x] Content types and validated custom fields.
  - [x] Site-scoped content type declarations use `PUT` upsert and opaque cursor listing.
  - [x] Declared custom fields validate required values, types, choices and unknown keys.
  - [x] Removing a declaration preserves existing content and its stored fields.
- [-] Posts/pages, drafts, revisions, slugs, scheduling and public reads.
  - [x] Content create/update/public lifecycle and immutable revision history API.
  - [x] Slug history preserves old published public paths after a slug change.
  - [ ] Scheduled publishing worker/queue and rollback/restore UX.
- [-] Taxonomy terms, trees, assignment and filtered listing.
  - [x] Site-scoped category/tag terms with language-aware slugs and opaque cursor listing.
  - [x] Category parent validation, recursive cycle protection and atomic content assignments.
  - [x] Content-to-term reads, replacement and term membership listing use canonical API contracts.
  - [ ] Taxonomy public archive URLs and localized fallback policy.
- [-] Media metadata, uploads, image variants, file storage and cleanup.
  - [x] Site-scoped file metadata, byte-sniffed allowlist, SHA-256 receipt and opaque cursor listing.
  - [x] Raw binary upload contract, local atomic file adapter and in-memory test adapter.
  - [x] RLS/composite keys, Cedar media grants, upload/trash audit receipts and durable cleanup tasks.
  - [ ] Image variants, authenticated/public binary download and orphan cleanup worker.

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
  - [ ] Sandboxed cloud compiler adapter, panel design screens and asynchronous build worker.
- [-] Forms, submissions, exports, spam controls and retention policy.
  - [x] Form declarations validate bounded fields, types, choices and site-local active slugs.
  - [x] Public submissions validate required/typed/known answers and return a receipt without exposing management metadata.
  - [x] Submission inbox uses `after`/`limit` cursors, unread filtering, mark-read and audited deletion.
  - [x] Forms/submissions use composite keys, RLS, Cedar grants and mutation audit receipts.
  - [ ] Export format, spam/rate-limit controls and scheduled retention worker.
- [-] Mail templates, delivery queue, retries and provider adapters.
  - [x] Strict site-scoped templates render bounded `{{variable}}` placeholders and expose preview without sending.
  - [x] Mailing lists/readers use normalized addresses, hashed unsubscribe tokens and explicit standing states.
  - [x] Transactional and campaign requests enqueue provider-neutral outbox rows; public/API requests never call a provider.
  - [x] Workers claim leases, record attempts, mark sent/retry/dead and support idempotency keys.
  - [x] Domain code uses the shared `Mailer` port and returns provider receipts without coupling to SMTP/cloud SDKs.
  - [ ] Concrete self-host/cloud providers, templated unsubscribe URL injection and rate-limit/deliverability policy.

## Commerce and learning

- [ ] Products, variants, prices, stock and site-local order numbering.
- [ ] Checkout, payment adapter, refunds, discounts and order audit.
- [ ] Courses, modules, lessons, video/file access and student enrollment.
- [ ] Expiring access, progress, completion and instructor permissions.

## Automation and collaboration

- [ ] Flows, triggers, steps, retries, idempotency and dead-letter handling.
- [ ] Jobs, site-scoped queue leases and worker execution.
- [ ] Boards, lists, cards, assignments, comments and activity history.
- [ ] Analytics events, aggregation, retention and export.
- [ ] Portable export/import with versioned manifests and validation.
- [ ] MCP resources/tools generated from the canonical API with grant checks.

## Runtime, panel and release

- [ ] Shared HTTP router with request-level site scope.
- [ ] Self-host configuration and upgrade path.
- [ ] Cloud/operator provisioning contract without direct database coupling.
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
  - [x] Forms declaration/submission PostgreSQL RLS isolation and HTTP validation/cursor/permission tests.
  - [x] Mail template/list/outbox PostgreSQL RLS state-machine tests and HTTP contract coverage.
  - [x] Identity HTTP integration covers setup, login, cursor, 401/403 and Cedar behavior.
  - [x] Generated OpenAPI/TypeScript/Rust/MCP artifacts have stale-contract tests.
  - [ ] Remaining domain HTTP suites.
- [ ] Release smoke test, migration rollback policy, backups and upgrade docs.
- [ ] Operator consumes only a tagged Mavi release/API contract.
