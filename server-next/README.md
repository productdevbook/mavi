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

## Workspace crates

| Crate | Responsibility |
| --- | --- |
| `mavi-core` | typed IDs, caller/site context, errors, grants, values and ports |
| `mavi-storage` | PostgreSQL pool, migrations and scoped transactions |
| `mavi-contract` | canonical endpoint declarations and contract validation |
| `mavi-runtime` | self-host/cloud runtime composition and site resolution |
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
| `mavi-mail` | strict templates, subscriber lists, unsubscribe tokens and provider-neutral outbox delivery |
| `mavi-shop` | site-scoped products, money, stock holds, coupons, checkout and order state transitions |
| `mavi-courses` | course authoring, ordered modules/lessons, isolated student sessions, enrollment, progress and protected lesson media |
| `mavi-jobs` | site-scoped durable queue leases, idempotency keys, retry backoff and dead-letter state |
| `mavi-flows` | validated trigger/step definitions, event fan-out, run snapshots and step history |
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
```

List inputs use opaque keyset cursors. The generated query contracts expose
`after` and bounded `limit`; page/offset inputs are not part of this workspace.
Forms use the same rule for both form declarations and submission inboxes.
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

Self-host stores binary objects outside PostgreSQL. Set `MAVI_FILES_DIR` to a
persistent directory (default: `./mavi-files`); object keys are generated from
file IDs and are always namespaced by `SiteContext.site_id`.

Design builds use the same `FileStore` boundary. The self-host baseline exposes
only `public/` source files through the static build engine; `src/` remains
non-executable source. Preview and live assets are immutable build artifacts,
and publish/rollback changes one site-scoped database pointer atomically.

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
