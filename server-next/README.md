# Mavi clean implementation

This workspace is the replacement implementation for the public Mavi CMS.
The existing `server/` remains available as a behavior and feature reference;
new code must not import it just to preserve an old boundary.

## Runtime boundary

Mavi owns one site's content, users, settings, media, commerce, courses,
automation and publishing. Mavi Operator owns organizations, accounts,
placements, billing, metering and cloud lifecycle. Operator may provision a
site, but site data is accessed through Mavi's site-scoped API and never by
reaching into Mavi's tables.

The same Mavi application runs in both modes:

- self-host uses a `FixedSiteResolver` and one configured site;
- cloud uses a request resolver and a shared shard pool;
- every application transaction receives a `SiteContext`;
- site-owned tables use `site_id` and scoped transactions;
- control-plane endpoints are explicit in the API contract.

The executable selects the runtime at startup. Self-host keeps the default
`MAVI_RUNTIME_MODE=fixed_site` and requires `MAVI_SITE_ID`. A cloud shard uses
one process, one router and one PostgreSQL pool for an allowlisted host
directory:

```text
MAVI_RUNTIME_MODE=shard
MAVI_SITE_HOSTS=www.example.com=<site-uuid>,store.example.com=<site-uuid>
```

`MAVI_SITE_HOSTS` is a validated startup snapshot: hosts are normalized,
duplicate claims are rejected and every mapped site is reconciled into the
shard catalog as active. The request `Host` is resolved into a `SiteContext`
before authentication and every domain transaction remains site-scoped. A
control-plane refresh must replace this snapshot through the deployment
boundary; the process never accepts a site ID supplied by a request.

Authentication endpoints also apply bounded site+action edge windows keyed by
the direct peer IP and a privacy-preserving User-Agent digest. The process
uses the socket peer by default. When a reverse proxy terminates connections,
only explicitly trusted proxy networks may supply the client IP:

```text
MAVI_TRUSTED_PROXY_CIDRS=10.0.0.0/8,192.0.2.0/24
```

Forwarded headers from any other peer are ignored. Raw IP addresses and
User-Agent values never enter the limiter buckets or security audit payloads;
the in-process adapter is bounded and records only the first edge-limit event
per source/action window.

## Workspace crates

| Crate | Responsibility |
| --- | --- |
| `mavi-core` | typed IDs, caller/site context, errors, grants, values and ports |
| `mavi-storage` | PostgreSQL pool, migrations and scoped transactions |
| `mavi-contract` | canonical endpoint declarations and contract validation |
| `mavi-runtime` | self-host/cloud runtime composition and site resolution |
| `mavi-http` | request admission, trusted edge signals, throttling and canonical HTTP composition |
| `mavi-identity` | setup, people, roles and password identity primitives |
| `mavi-content` | content entries, publication state and site-declared content types |
| `mavi-settings` | site settings, timezone and site language configuration |
| `mavi-authz` | embedded Cedar policy evaluation with site-scope enforcement |
| `mavi-files` | atomic local and in-memory site-scoped binary storage adapters |
| `mavi-media` | file metadata, byte detection, upload/trash orchestration and media API |
| `mavi-audit` | immutable site-scoped mutation receipts and cursor-filtered audit reads |
| `mavi-trash` | shared trash listing, restore, permanent deletion and media cleanup policy |
| `mavi-design` | site-owned source files, immutable preview builds, publish/rollback and public asset metadata |
| `mavi-forms` | validated site form declarations, public submissions and cursor-based inbox management |
| `mavi-mail` | strict templates, subscriber lists, unsubscribe tokens and a provider-neutral outbox with sealed security messages |
| `mavi-shop` | site-scoped products, money, stock holds, coupons, checkout and order state transitions |
| `mavi-courses` | course authoring, ordered modules/lessons, isolated student sessions, enrollment, progress and protected lesson media |
| `mavi-jobs` | site-scoped durable queue leases, idempotency keys, retry backoff and dead-letter state |
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
the Mavi release, canonical API fingerprint, storage schema version, fixed-site
or shard mode, and pagination policy. The pagination policy is deliberately
cursor-only (`after`, bounded `limit`, opaque `next_cursor`); page numbers and
offsets are not accepted or advertised.

List inputs use opaque keyset cursors. The generated query contracts expose
`after` and bounded `limit`; page/offset inputs are not part of this workspace.
JSON and query object inputs are closed by default: an unknown top-level field
returns `400` with `error.code = "unknown_field"` and the offending field path.
Domain-owned maps such as content fields and flow configuration remain open
only where their schema explicitly says so.
Forms use the same rule for both form declarations and submission inboxes.
Site settings store an optional normalized canonical HTTP(S) URL; query strings,
fragments and userinfo are refused, and PATCH can explicitly set or clear the
value. Public content resolution first tries the requested language, then its
regional base tag (for example `de-DE` to `de`), and finally the site's
configured default language.
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

Automation keeps panel definitions separate from worker execution. Flow events
enqueue registered site jobs in the producer transaction; workers claim a
short lease, execute outside the database transaction, and finish or fail only
while that lease is still theirs. Repeated source events use an idempotency key,
run definitions are snapshotted, and exhausted attempts remain visible as dead
letters. The canonical automation and job management APIs expose only opaque
keyset cursors.

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

The private operator relocation envelope extends that snapshot with identity
credential hashes, live media metadata/bytes and design/build state. Credentials
are redacted from debug output; sessions and API keys are revoked at the target
rather than copied. Media and design artifact bytes stay behind the site-scoped
`FileStore` and are verified by size and SHA-256 before import; publish pointers
are restored only after their referenced builds exist.

Self-host stores binary objects outside PostgreSQL. Set `MAVI_FILES_DIR` to a
persistent directory (default: `./mavi-files`); object keys are generated from
file IDs and are always namespaced by `SiteContext.site_id`. Uploads are
private by default; an explicit `visibility=public` upload query is required
before `/public/v1/files/{id}` can serve bytes. Authenticated callers with the
media view grant use `/api/v1/files/{id}/content`. Both paths verify the stored
byte count and SHA-256 receipt before responding.

Design builds use the same `FileStore` boundary. The self-host baseline exposes
only `public/` source files through the static build engine; `src/` remains
non-executable source. Preview and live assets are immutable build artifacts,
and publish/rollback changes one site-scoped database pointer atomically.

MCP follows the current stateless `2026-07-28` transport shape: the server
advertises its supported protocol through `server/discover`, every request is
self-contained, and no session or `initialize` handshake is part of the new
runtime contract. `tools/list` uses MCP's opaque cursor, while `tools/call`
routes back through the canonical HTTP handlers. Tool descriptors remain
generated from the same API catalog and execution is subject to the endpoint's
Cedar grant.

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
