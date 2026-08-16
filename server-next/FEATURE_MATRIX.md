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
- [x] Fixed-site runtime composition for self-host.
- [x] Request admission middleware creates and validates `SiteContext`.
- [ ] Cloud shard runtime resolves a site without a per-site router/process.
- [ ] OpenAPI, TypeScript client and MCP tool generation from the same contract.
- [ ] RLS/DB guard, composite foreign keys and site-aware unique constraints across every domain table.
- [ ] Migration and schema integration tests against PostgreSQL.

## Setup, identity and access

- [ ] First-run setup and site bootstrap.
- [ ] People, account sessions, password recovery and API keys.
- [ ] Roles, grants, assistant delegation and revocation.
- [ ] Student identity isolated from panel accounts.
- [ ] Rate limits, request audit identity and security events.

## Site configuration and content

- [ ] Site settings, URL, timezone and language configuration.
- [ ] Content types and validated custom fields.
- [-] Posts/pages, drafts, revisions, slugs, scheduling and public reads.
- [ ] Taxonomy terms, trees, assignment and filtered listing.
- [ ] Media metadata, uploads, image variants, file storage and cleanup.

## Operations and publishing

- [ ] Audit receipts for every mutation and auditable actor attribution.
- [ ] Trash, restore and permanent deletion policy.
- [ ] Design files, preview builds, publish, rollback and public serving.
- [ ] Forms, submissions, exports, spam controls and retention policy.
- [ ] Mail templates, delivery queue, retries and provider adapters.

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
- [ ] Release smoke test, migration rollback policy, backups and upgrade docs.
- [ ] Operator consumes only a tagged Mavi release/API contract.
