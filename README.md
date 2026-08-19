# Mavi

A content management system you run yourself. The clean rewrite is one Rust
binary and one PostgreSQL database: site-scoped content, identity, media,
publishing, forms, mail, commerce, courses, automation and MCP.

The public panel is being regenerated from the clean canonical API. Until that
panel slice lands, the published image is the API runtime and the old `client/`
and `server/` workspaces remain reference material, not a mixed deployment.
Mavi is a CMS, not a hosting business; organization, billing, metering and
shard lifecycle belong in `mavi-operator`.

MIT. Run it, change it, sell it.

- **Clean site boundary** — self-host uses one `FixedSiteResolver`; cloud uses
  one shared shard router and resolves the site from an allowlisted host.
- **Canonical API** — `/api/v1`, `/public/v1` and `/mcp` are described once and
  generate OpenAPI, TypeScript/Rust artifacts and MCP tool metadata.
- **Cursor-only lists** — every public list uses opaque keyset cursors; page
  numbers and offsets are not part of the contract.
- **Scoped storage** — every site-owned transaction sets PostgreSQL scope;
  composite keys and forced RLS protect the database boundary.
- **Observable runtime** — `/healthz`, `/readyz` and Prometheus `/metrics` are
  global operational endpoints, outside site admission.
- **Everything is written down** — mutations are audited and background work
  uses fenced, site-scoped queue leases.

## Quick start

```bash
curl -O https://raw.githubusercontent.com/productdevbook/mavi/main/docker-compose.yml
curl -O https://raw.githubusercontent.com/productdevbook/mavi/main/Caddyfile
{
  echo "POSTGRES_PASSWORD=$(openssl rand -base64 24)"
  echo "MAVI_KEYS=1:$(openssl rand -base64 32)"
  echo "MAVI_SITE_ID=$(uuidgen)"
} > .env
docker compose up -d
```

`MAVI_SITE_ID` is the durable identity of this self-hosted site's rows. Keep it
stable across upgrades. `MAVI_KEYS` seals credentials and must also survive
restarts. The API starts with the fixed-site runtime and runs migrations before
opening its listener.

The clean image currently exposes the API, not the unfinished legacy panel.
For example, setup is available at:

```bash
curl -sS -X POST http://localhost/api/v1/setup \
  -H 'content-type: application/json' \
  -d '{"site_name":"Example","email":"owner@example.com","name":"Owner","password":"change-this-password"}'
```

On a public machine set `MAVI_DOMAIN=example.com`, point DNS at the machine and
let Caddy terminate TLS. Any trusted reverse proxy may be used instead; pass
the `Host` header through and configure `MAVI_TRUSTED_PROXY_CIDRS` when it
supplies forwarded client signals.

The compose file runs a bundled Postgres. To use your own, set `DATABASE_URL`
and drop the `postgres` service — which is also what stops `POSTGRES_PASSWORD`
being asked for:

```bash
DATABASE_URL=postgres://user:password@your-host:5432/mavi docker compose up -d
```

### Images

| | |
|---|---|
| API | `ghcr.io/productdevbook/mavi` |
| Panel | Not included until the clean generated-client slice lands |

Both are built for `linux/amd64` and `linux/arm64`.

### Configuration

The clean API reads these at its binary boundary:

| Variable | Default | Notes |
|---|---|---|
| `DATABASE_URL` | — | PostgreSQL. Required. |
| `MAVI_KEYS` | — | What seals a site's secrets. `1:<thirty-two bytes, base64>`, and a version and comma for each older key. Required; the process refuses to start without it, and refuses to start on one it cannot read rather than making one up. |
| `MAVI_SITE_ID` | — | Fixed-site UUID. Required and stable for the lifetime of the installation. |
| `MAVI_RUNTIME_MODE` | `fixed_site` | `fixed_site` for self-host; `shard` is the cloud-shaped runtime. |
| `MAVI_FILES_DIR` | `./mavi-files` / `/data/files` in the image | Persistent site-scoped binary storage. |
| `LISTEN` | `0.0.0.0:8080` | HTTP listener address. |
| `DATABASE_CONNECTIONS` | `10` | PostgreSQL pool size. |
| `MAVI_WORKER_ID` | generated default | Site-worker identity for lease fencing. |
| `MAVI_WORKER_LEASE_SECONDS` | worker default | Queue lease duration. |
| `MAVI_WORKER_POLL_MILLIS` | worker default | Queue poll interval. |
| `MAVI_TRUSTED_PROXY_CIDRS` | none | Explicit proxy networks allowed to provide forwarded client IPs. |
| `RUST_LOG` | `info` | |

## Self-host and cloud boundary

Self-host is one fixed site, selected by `MAVI_SITE_ID` and admitted through a
`FixedSiteResolver`. Cloud hosting is not part of this repository: the private
operator owns organization and shard lifecycle, and mounts the same Mavi
router with an allowlisted host-to-site snapshot.

Both modes use the same site-scoped application services and PostgreSQL
transactions. No request can select an arbitrary site ID, and no cloud mode
constructs a router or process per site. See the clean workspace
[`server-next/README.md`](server-next/README.md) for the runtime and contract
details.

## More than posts

A site is not always a blog. **Content types** in the panel say what this one
publishes: every site has posts and pages, and a site adds its own when what it
publishes has facts of its own — a course with a price and a level, a property
with rooms.

What a kind declares is what may be written: a field nothing declared is
refused rather than quietly kept, and a number that is not a number is refused
too. What was written under a field the kind no longer has is kept as it was,
and comes back if the field does.

Those fields are also what a front end asks about:
`/api/posts?type=recipe&field=minutes&at_most=30` is every recipe under thirty
minutes, and a field nothing declared is refused rather than matching nothing.

## Publishing

A design is rows on a draft: what a site looks like is written to `src/` and
`public/`, built by whatever this machine is configured to build with, and put
live when somebody presses publish. Before that it can be built to an address
to look at — a preview, billed the same as a publish, that leaves what is live
alone.

A build that fails leaves what is live alone as well, because half a site is
worse than an old one.

A post given a state of **scheduled** and a moment goes out when that moment
arrives — the machine looks every minute — and whatever was waiting for it is
told.

## Teaching

Courses hold modules and lessons; a lesson plays a video the site uploaded.
Somebody is put on a course for as long as the site says, and access that was
sold for ninety days stops opening the course after ninety days. What they
finished stays finished, and letting them back in is one call rather than an
enrolment written again.

A student is not a panel account: they sign in at the site's own front, hold no
grants at all, and reach nothing in the panel.

## Assistants

Every site answers the [Model Context Protocol](https://modelcontextprotocol.io)
at `https://your-site/mcp`.

An assistant is handed a key from **API** in the panel: it carries the grants
of whoever handed it over, expires by itself, and can be taken back. Nothing is
written with it that the record does not say was written by an assistant.

What it can do is what the panel can do, through the same grants — reading and
writing posts, filing them, uploading, reading what has come in through a form,
looking at orders, and working on the design. What it cannot do is publish:
that is a person's, and there is no tool for it.

## Connecting a front end

Every site publishes an `llms.txt` describing itself, and the API describes
itself at `/openapi.json`. The panel's own TypeScript types are generated from
that description, and a test fails while they are stale — so a path this build
does not serve is a type error rather than a 404 somebody finds later.

## Moving a site here

Nothing here moves a site from another machine: that was the mover, and it
belongs with the half that runs many machines rather than with the CMS. What
this does have is [taking a copy](#a-copy-of-what-is-written) — the languages,
what things are filed under, and everything written — from the panel's own
settings, which is enough to carry a site somewhere else by hand.

## Development

The clean runtime is the `server-next/` workspace. The old `server/` and
`client/` workspaces are kept as behavior/reference material while the panel is
regenerated from the v1 contract.

```bash
cd server-next
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --doc
cargo run -p mavi-http --bin generate_contract -- fingerprint
```

The tests want a PostgreSQL, because a site is rows in one and a test of what a
site holds should be asked of one:

```bash
docker run -d --name mavi-test-db -p 127.0.0.1:5433:5432 \
  -e POSTGRES_PASSWORD=test -e POSTGRES_DB=mavi_test postgres:18-alpine
export TEST_DATABASE_URL=postgres://postgres:test@127.0.0.1:5433/mavi_test
```

Each shape is migrated once into a template and every test is handed a copy, so
nothing has to be run beforehand.

Or run the whole thing in containers, built from your checkout:

```bash
docker compose -f docker-compose.dev.yml up --build
```

### Layout

```
server-next/         the clean API/runtime rewrite
  mavi-core/         typed IDs, scope, errors, grants and ports
  mavi-storage/      scoped PostgreSQL transactions and migrations
  mavi-contract/     canonical endpoint declarations and generators
  mavi-http/         request admission and API composition
  mavi-runtime/      fixed-site and shared-shard runtime boundaries
  mavi-<domain>/     one application/service boundary per site feature
server/              legacy workspace, reference only
client/              legacy panel, reference until generated-client rewrite
wordpress-plugin/    the WordPress migration plugin (GPLv2+)
```

The panel is English and Turkish, via [Lingui](https://lingui.dev). Which
language the panel is read in has nothing to do with which languages the site
writes in.

## The parts that need more than a paragraph

| | |
|---|---|
| [ports.md](docs/ports.md) | what this software asks a host for, and why there is no plugins table |
| [describing.md](docs/describing.md) | how the API describes itself, and what the panel is generated from |
| [assistant.md](docs/assistant.md) | what an assistant can do here, and why there is no list of tools |
| [serving.md](docs/serving.md) | what a visitor sees, and why a build is a folder and going live is a row |
| [media.md](docs/media.md) | where uploaded pictures are kept |
| [flows.md](docs/flows.md) | what a site does on its own when something happens |
| [boards.md](docs/boards.md) | what a site works through in stages |
| [commerce.md](docs/commerce.md) | selling things |
| [courses.md](docs/courses.md) | selling courses, and why a student is not a panel account |
| [video.md](docs/video.md) | putting a lesson's video somewhere that is not this machine |

## License

MIT — see [LICENSE](LICENSE). The WordPress plugin is GPL-2.0-or-later, as
WordPress plugins must be.

Every dependency has been checked against that, and what was deliberately not
borrowed is written down too: [LICENSES.md](LICENSES.md).

## What this is not

It is not a hosting business, and the parts that make one are not here:
metering what each site uses, billing for it, making and unmaking sites on a
machine, moving one between machines, a console that reads across all of them.
That is somebody's product, and this is the CMS such a product would run.

The seam it is built on is real rather than a promise:
`old/src/kernel/outside.rs` let a crate that depended on this one hand in
its own endpoints and its own kinds of queued work, mounted through the same
guard, the same rate limit and the same audit rule as everything here. Nothing
mounted that way can skip a permission check or a receipt, and a test says so.

It is also not a plugin marketplace. What a site can be made to talk to — its
mail server, its payment provider — is a decision in the software rather than
a form somebody fills in, and adding a third is a change to this repository.
