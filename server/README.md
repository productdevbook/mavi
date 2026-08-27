# Mavi clean implementation

This is the public Mavi CMS server. It owns the canonical site-scoped API and
runtime; new code must not reintroduce the removed legacy workspace just to
preserve an old boundary.

## Runtime boundary

Mavi owns one site's content, users, settings, media, commerce, courses,
automation and publishing. A tenant/control-plane repository owns
organizations, billing, placement, backups and lifecycle, and talks to Mavi
only through its container and versioned HTTP/OpenAPI boundary.

The executable requires one `MAVI_SITE_ID`, one PostgreSQL database and one
file namespace. Host-to-site routing, shard runtime, relocation and
control-plane lifecycle code are not part of this process. Every application
transaction still receives a `SiteContext`; `site_id`, PostgreSQL RLS,
composite foreign keys and site-bound encryption remain the data-isolation
boundary.

Startup is ordered as migrations, single-site preflight, configured-site
ensure, compiled plugin registry/Cedar validation, Hatchet bridge validation,
then listener bind. `MAVI_PROCESS_ROLE=all` runs HTTP, the Rust executor and
the workflow outbox relay; `api` serves HTTP only, while `worker` exposes only
the private Rust executor and outbox relay on `MAVI_EXECUTOR_LISTEN`.

Authentication endpoints and public form submissions apply bounded site+action
edge windows keyed by the direct peer IP and a privacy-preserving User-Agent
digest. The process uses the socket peer by default. When a reverse proxy
terminates connections, only explicitly trusted proxy networks may supply the
client IP:

```text
MAVI_TRUSTED_PROXY_CIDRS=10.0.0.0/8,192.0.2.0/24
```

Forwarded headers from any other peer are ignored. Raw IP addresses and
User-Agent values never enter the limiter buckets or security audit payloads;
the in-process adapter is bounded and records only the first edge-limit event
per source/action window.

## Self-host image

The API release image is built from this workspace only. It runs as a non-root
user, persists site files below `/data/files` and listens on `0.0.0.0:8080`.
The panel is a separate image built from the same release tag; the self-host
compose package mounts it at `/admin`, `/learn` and `/shop` without putting
browser assets in the Rust runtime image.

For a local image build:

```bash
docker compose -f ../docker-compose.dev.yml up --build
```

For a published image, use the root `docker-compose.yml` and pin
`MAVI_VERSION` to a release tag. Keep `MAVI_SITE_ID`, `MAVI_KEYS`, the
PostgreSQL data and `/data` stable across upgrades. The binary applies pending
SQL migrations before it starts serving traffic; a failed migration prevents
the listener from opening.

Operational probes are global and do not require a site `Host`: `/healthz`
reports process liveness, `/readyz` checks the shared database, and `/metrics`
exposes process-local HTTP, worker and low-cardinality Cedar authorization
counters in Prometheus text format.

## Workspace crates

| Crate | Responsibility |
| --- | --- |
| `mavi-core` | typed IDs, caller/site context, errors, grants, values and ports |
| `mavi-storage` | PostgreSQL pool, migrations and scoped transactions |
| `mavi-contract` | canonical endpoint declarations and contract validation |
| `mavi-runtime` | fixed single-site runtime composition |
| `mavi-application` | use-case orchestration, plugin lifecycle, Cedar entry point, workflow outbox and cross-domain trash policy |
| `mavi-http` | request admission, plugin gates, trusted edge signals, throttling and HTTP transport |
| `mavi-identity` | setup, people, roles and password identity primitives |
| `mavi-content` | content entries, publication state and site-declared content types |
| `mavi-settings` | site settings, timezone and site language configuration |
| `mavi-authz` | embedded Cedar policy evaluation with site-scope enforcement |
| `mavi-files` | atomic local and in-memory site-scoped binary storage adapters |
| `mavi-media` | file metadata, byte detection, upload/trash orchestration and media API |
| `mavi-observability` | process-local HTTP/worker/Cedar decision counters and Prometheus exposition primitives |
| `mavi-audit` | immutable site-scoped mutation receipts and cursor-filtered audit reads |
| `mavi-trash` | compatibility re-export for the application-owned trash policy |
| `mavi-design` | site-owned source files, immutable preview builds, publish/rollback and public asset metadata |
| `mavi-forms` | validated site form declarations, public submissions, cursor-based inbox management and versioned bounded export |
| `mavi-feedback` | bounded site-scoped panel reports with cursor reads and transactional audit receipts |
| `mavi-mail` | strict templates, subscriber lists, unsubscribe tokens and a provider-neutral outbox with sealed security messages |
| `mavi-shop` | site-scoped products, money, stock holds, coupons, checkout and order state transitions |
| `mavi-courses` | course authoring, ordered modules/lessons, isolated student sessions, enrollment, progress and protected lesson media |
| `mavi-worker` | workflow outbox relay and private Rust executor behind the Hatchet bridge |
| `integrations/hatchet-worker` | official Hatchet Go SDK adapter; no business logic |
| `mavi-flows` | validated trigger/step definitions, event fan-out, run snapshots and step history |
| `mavi-boards` | ordered site-scoped boards, lists, cards, assignments, comments and immutable activity |
| `mavi-analytics` | bounded privacy-preserving events, daily rollups, cursor export and retention |
| `mavi-portable` | versioned site bundles with schema hashes, validation and atomic import strategies |
| `mavi-sealing` | AES-256-GCM keyring adapter with site-bound authenticated ciphertext |
| `mavi-secrets` | site-scoped provider credential lifecycle, sealing boundary and metadata-only API |
| `mavi` | executable composition root |

Domains are added only after the foundation is stable. Each domain owns its
application service, repository, migration, API declarations, Cedar action
mapping and tests.

## Generated contract artifacts

`mavi-contract` is the only source of API shape metadata. The HTTP composition
root exposes the generated OpenAPI document at `/openapi.json`, and the
committed snapshots under [`mavi-http/contracts`](mavi-http/contracts) are
checked in CI:

```bash
cargo run -p mavi-http --bin generate_contract -- openapi
cargo run -p mavi-http --bin generate_contract -- typescript
cargo run -p mavi-http --bin generate_contract -- rust
cargo run -p mavi-http --bin generate_contract -- mcp
cargo run -p mavi-http --bin generate_contract -- fingerprint
```

After provisioning, an operator or panel checks `/api/v1/runtime/manifest`.
The response is the compatibility boundary for a running site: it identifies
the Mavi release, canonical API fingerprint, storage schema version, active
compiled plugins and pagination policy. The pagination policy is deliberately
cursor-only (`after`, bounded `limit`, opaque `next_cursor`); page numbers and
offsets are not accepted or advertised.

List inputs use opaque keyset cursors. The generated query contracts expose
`after` and bounded `limit`; page/offset inputs are not part of this workspace.
JSON and query object inputs are closed by default: an unknown top-level field
returns `400` with `error.code = "unknown_field"` and the offending field path.
Domain-owned maps such as content fields and flow configuration remain open
only where their schema explicitly says so.
Forms use the same rule for both form declarations and submission inboxes.
Each form's `kept_days` is enforced by the shared site-scoped worker through
an idempotent daily `forms.retention` job; expired answers are redacted behind
a submission tombstone and the retention count is recorded as a system audit
receipt in the same transaction. Authenticated form managers can export active
submissions through `/api/v1/forms/{id}/submissions/export`; the response is a
bounded `mavi.forms.submissions` version 1 JSON envelope with an opaque cursor,
the form declaration and an auditable read. Deleted or retention-redacted rows
never appear in this export.
Audit managers can download a chronological, filtered `/api/v1/audit/export`
envelope. It is site-scoped, capped at 10,000 events per request, and records
the export access as `audit.events.exported`; retention policy remains an
explicit operations decision rather than an implicit destructive default.
Core content, media files and taxonomy terms use the site setting
`trash_retention.days` (1–3,650 days) and an idempotent daily
`trash.retention` worker. Permanent deletion and its system audit receipt are
transactional; media bytes and generated variants continue through the
durable `FileStore` cleanup job. Domain-specific trash for forms, shop,
courses, boards and flows remains an explicit follow-up rather than being
silently treated as core content trash.
Site settings store an optional normalized canonical HTTP(S) URL; query strings,
fragments and userinfo are refused, and PATCH can explicitly set or clear the
value. Public content resolution first tries the requested language, then its
regional base tag (for example `de-DE` to `de`), and finally the site's
configured default language.
Public taxonomy archives use `/public/v1/terms/{kind}/{slug}` and apply the same
language candidates before returning only published content through the shared
opaque cursor page contract.
Public submission delivery is intentionally behind the existing `Mailer` port;
provider selection, retries and an outbox worker belong to the mail/automation
slice and are not performed inline in the public request. Mail templates render
strict `{{variable}}` placeholders, subscriber tokens are stored only as hashes,
and delivery workers claim short leases before calling a provider adapter. Shop
checkout uses site-local order numbers, immutable line snapshots and
email-scoped idempotency keys; public product responses never reveal stock
counts.

Courses keep panel accounts and students as different principals. Panel course
operations require the Cedar `courses` capability; students receive a
single-use invitation, activate an expiring session, and can only read lessons
and attached media for their own enrollment while the course is open. Course,
student, enrollment and progress lists use the same opaque keyset cursor rule;
offset/page-number pagination is not supported.

Automation keeps panel definitions separate from worker execution. New domain
mutations write a small workflow intent atomically with their database change;
the outbox relay publishes it to Hatchet, which owns retries, timeouts,
concurrency, rate limits, priority and run control. Hatchet calls the private
Rust executor with IDs and idempotency keys only. The small compatibility
projection used by existing domain ports lives inside `mavi-application` and
writes only the canonical workflow tables; there is no separate jobs crate or
second queue. The historical `jobs` table is decommissioned by a forward
migration and is not used at runtime.
The canonical automation and workflow APIs
expose only opaque keyset cursors. Mail still uses the injected `Mailer` port
when configured, and provider credentials never enter site rows.

Outbound sender identity is deployment policy, not an arbitrary request field.
`MAVI_MAIL_FROM` is required when the gateway is enabled,
`MAVI_MAIL_FROM_NAME` supplies its optional display name, and
`MAVI_MAIL_ALLOWED_SENDER_DOMAINS` is a comma-separated allowlist. A site may
configure a sender only within that allowlist. Mavi stores the validated sender
on the site and copies it onto each queued delivery before the worker runs.

Provider callbacks use a separate `MAVI_MAIL_WEBHOOK_INGEST_TOKEN` and the
`POST /internal/v1/mail/provider-events` contract. The trusted gateway
normalizes vendor-specific events into `delivered`, `bounced` (transient or
permanent) and `complained` records. Event IDs are site-scoped and idempotent;
permanent bounces and complaints move the reader to a suppression standing and
cancel queued campaign deliveries for that address. Transactional messages
are not suppressed by list standing, and transient bounces are recorded without
permanent suppression. The inbound credential is separate from the outbound
gateway credential so a provider cannot reuse the wrong trust direction.

Provider credentials are a separate site-scoped domain. The API can create,
rotate, list and revoke only credential metadata; values are sealed through the
`Seals` port and are available only to trusted provider adapters. Self-host
must provide `MAVI_KEYS` as an ordered keyring such as
`1:<base64-32-byte-key>,2:<older-base64-32-byte-key>`. New ciphertext uses the
first key, while older keys remain readable during rotation. The site ID is
authenticated data, so copying ciphertext between sites fails closed.

Boards use integer positions and transactional reindexing for drag-and-drop;
floating-point midpoint positions are not part of the new contract. Card moves,
assignments and comments are site-scoped and write both an audit receipt and
append-only activity history. Analytics ingestion deliberately accepts no
arbitrary properties, visitor fingerprint, IP address or query string. Raw
events are an export surface with bounded retention, while daily aggregates are
the stable reporting surface; both use opaque keyset cursors.

Portable bundles are explicit application snapshots rather than database dumps.
Version 2 carries site settings/languages (including the optional canonical
site URL), content type declarations, taxonomy, content and revision history,
old slug paths and term assignments. Every bundle
contains source-site provenance, record counts and a schema hash. Import first
validates references and conflicts, then applies in one site-scoped transaction
using `validate_only`, `create_only` or `upsert` semantics.

Site movement, backup orchestration and reprovisioning belong to the external
tenant repository. Mavi's portable bundle is a content/settings snapshot only;
it is not a relocation envelope and never copies sessions, API keys or provider
secret values. Media and design artifact bytes stay behind the site-scoped
`FileStore` and are handled by the instance's normal export/import workflow.
Image variants remain derived data and are regenerated by the workflow worker.

Self-host stores binary objects outside PostgreSQL. Set `MAVI_FILES_DIR` to a
persistent directory (default: `./mavi-files`); object keys are generated from
file IDs and are always namespaced by `SiteContext.site_id`. Uploads are
private by default; an explicit `visibility=public` upload query is required
before `/public/v1/files/{id}` can serve bytes. Authenticated callers with the
media view grant use `/api/v1/files/{id}/content`; generated image variants are
listed at `/api/v1/files/{id}/variants` and served through authenticated or
source-visibility-checked public variant paths. All media paths verify the
stored byte count and SHA-256 receipt before responding.

Design builds use the same `FileStore` boundary. The self-host baseline exposes
only `public/` source files through the static build engine; `src/` remains
non-executable source. Preview and live assets are immutable build artifacts,
and publish/rollback changes one site-scoped database pointer atomically.

MCP follows the current stateless `2026-07-28` transport shape: the server
advertises its supported protocol through `server/discover`, every request is
self-contained, and no session or `initialize` handshake is part of the new
runtime contract. `tools/list` uses MCP's opaque cursor, while `tools/call`
routes back through the canonical HTTP handlers. Tool descriptors remain
generated from the same active API catalog and execution is subject to the
endpoint's Cedar grant.

## Plugins and durable workflows

`mavi-application` owns the compiled `PluginRegistry`. The database stores only
`site_plugins` activation/configuration rows; it never loads native code.
Fresh setup enables `core` and `writing`. Plugin activation is owner-only,
dependency-checked, audited and broadcast with PostgreSQL `NOTIFY`. Disabled
plugins return HTTP 404 and disappear from runtime OpenAPI, MCP and panel
navigation without deleting their data.

Domain mutations write a small `WorkflowIntent` to `workflow_outbox` in the
same `SiteTx` as the mutation. `mavi-worker` claims and retries the outbox,
then the private `integrations/hatchet-worker` process publishes a Hatchet
workflow. Hatchet owns retry, timeout, schedule, concurrency, rate-limit,
priority, cancellation and run history; Rust remains the business-logic
executor and uses idempotency keys for at-least-once delivery.

Future-dated intents use Hatchet's schedule API and the bridge provisions a
site-scoped maintenance cron (`MAVI_HATCHET_MAINTENANCE_CRON`) for retention
and discovery work. Scheduled resources are stored in the same local run ID
column with a `schedule:` prefix so cancellation and replay use the matching
Hatchet control API.

The bundled Hatchet compose server exposes plaintext gRPC inside the compose
network, so the bridge defaults `MAVI_HATCHET_TLS_STRATEGY` to `none`. A bridge
pointing at secured external Hatchet must set `tls` or `mtls` and provide the
official SDK's corresponding certificate environment settings.

## Dependency policy

Direct Rust dependencies are pinned to the latest compatible releases when
the workspace is refreshed. `Cargo.lock` is committed. Refresh and verify with:

```bash
cargo update
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The Rust toolchain used for a refresh must be recorded in the change. A new
major version is adopted deliberately when its migration is understood; a
stale version is never retained merely because the old implementation used it.

## Delivery order

See [`FEATURE_MATRIX.md`](FEATURE_MATRIX.md) for the complete feature and
acceptance checklist. The implementation order is foundation, setup/auth,
content, taxonomy, media, audit/trash, design/publish, then the remaining
domains. Operator integration starts only after the Mavi API and release
contract are stable.
