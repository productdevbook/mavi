# server

A second backend, written from the beginning, in one crate. Nothing in
`backend/` is treated as a decision already made; where this differs, the reason
is written down here.

The kernel came first — the parts every domain has to go through — because the
failure this rewrite exists to prevent is not architectural. It is writing a
domain, shipping it, and remembering a week later that it never wrote an audit
row, never emitted an event, and asked Postgres for rows without saying which
tenant was asking. Every domain since has gone through that gate, and the tests
that hold it shut are the ones worth reading first.

## Running it

    docker compose up

One process that is the API and its workers, and a Postgres beside it. A
machine that serves other people's sites splits the two — `ROLE=api` and
`ROLE=worker` — and gives the workers no port.

What it reads from the environment:

| | |
|---|---|
| `DATABASE_URL` | required |
| `MAVI_KEYS` | `1:<base64>`; required, because a machine that makes one up cannot read what the last one sealed. Set and unreadable refuses to start rather than making one up. `MAVI_INVENT_KEYS=yes` is the way to say that nothing here is worth keeping |
| `ROLE` | `api`, `worker`, or unset for both |
| `WORKERS` | how many take work at once; four |
| `PROXY_HOPS` | how many proxies rewrite the forwarded-for header; zero, and zero means the header is not believed at all |
| `UPLOADS_DIR` | where the bytes go |
| `SMTP_URL`, `MAIL_FROM` | the machine's own mail server, for sites that have not plugged in their own |
| `PAYMENTS_*`, `BUILDER_*`, `TRANSCODER_*` | who does the work that is not this process's |

`/healthz` says the process is alive. `/readyz` asks the database, which is
what decides whether traffic should arrive. `/metrics` is what to scrape.

## The decisions

**One schema, with `tenant_id` and row-level security.** Not a schema per
tenant. A schema per tenant makes a migration an operation over N schemas that
can half-succeed, puts DDL on the request path, and gives the connection pool
one hot pool per tenant. One schema makes a migration a single transaction and
makes isolation something Postgres enforces rather than something every query
has to remember.

**Isolation is structural, not remembered.** A database handle comes from
`Db::tenant`, which opens a transaction and sets `app.tenant_id` on it before
returning a `TenantConn`. Every tenant-scoped table has an RLS policy reading
that setting, and forces it, so it applies to the table's owner too. A query
that forgets its `where tenant_id` sees nothing rather than seeing everyone.
Reaching across tenants is `OperatorConn` — a different type, named so that
every use of it can be found.

**Authorization is a Cedar policy set, not a matrix in Rust.** Default deny: a
resource no `permit` covers belongs to nobody, so a handler that forgets to ask
fails closed. Roles are data — a role carries a set of grants the site edits,
and the policy asks only whether the grant needed is held, so a new role is not
a deploy. A hold on a site closes every access that changes something, from
inside the policy, without a handler having to know holds exist. The policies
are validated against a schema in CI, and each has a test that says who it lets
in and who it does not.

**A route declares its guard or does not compile.** `Endpoint` takes a `Guard`,
and a struct literal has to name every field: who it is for, what it needs, and
what rate limit it carries. A public route says so out loud, and there is a test
listing every one of them.

**One crate, modules by domain.** `src/orders/`, `src/tenants/`, not `core`,
`http`, `mcp`. The split in `backend/` produced five dependency cycles and a
`dto/` nobody could place a type in. A workspace split is justified when a
second binary genuinely needs a subset, and not before.

**sqlx with SQL migrations, no ORM.** The things this schema depends on —
RLS policies, check constraints, `jsonb`, `FOR UPDATE SKIP LOCKED`, partial
indexes — are written in SQL either way. A mapping layer over a database that is
never going to be swapped buys portability nobody wants and hides the plan.

**The queue is a table.** `SELECT ... FOR UPDATE SKIP LOCKED`, a visibility
timeout, an attempt counter, a dead letter. Enqueue happens inside the caller's
transaction, so work is never scheduled for something that then rolled back.
The flow runner and outbound webhook delivery are both users of it; there is no
separate stateful service to run.

**Outbound webhooks follow Standard Webhooks.** Signature, id, timestamp. At
least once, with the id being how a receiver dedupes. Delivery attempts are
rows, so the panel can show what was sent, what came back, and offer to send it
again.

## Layout

    server/
      migrations/       plain SQL, applied in order, once
      src/kernel/       what every domain goes through
      src/<domain>/     one module per domain, with its own README
      DOMAIN_TEMPLATE.md

## The domains

| | What it is |
|---|---|
| `console` | the machine's own screens: sites, and whoever runs it |
| `auth` | signing in, signing out, saying who you are |
| `people` | who is on a site, what they may do, how they arrive |
| `content` | posts, pages, categories, tags, redirects |
| `media` | what a site has uploaded, and how it is served |
| `forms` | a site's forms and what people send through them |
| `shop` | what a site sells, and what happens when somebody buys |
| `mail` | lists, subscribers, campaigns |
| `learning` | courses, and the students taking them |
| `boards` | a board, its columns, and the cards on them |
| `flows` | what a site arranged to happen when something happens |
| `publishing` | the project a site is built from, and putting it live |
| `analytics` | what a site was asked for, without keeping who asked |
| `usage` | what a site used, and what it is charged |
| `site` | a copy of what is held about somebody, and taking it away |
| `mcp` | a second surface onto the same data, under the same policy |
| `webhooks` | delivering what the outbox holds |
| `pages` | what is wrong with a page before anybody reads it |
| `plugins` | what a site plugs into: its own mail server, its own provider |
| `transfers` | moving a site, as something that survives being killed |
| `trash` | what was thrown away, and putting it back |
| `portable` | a site as a bundle, and reading one back |
| `assistant` | a key an assistant can work with, good for a day |
| `health` | whether a site is well, and whether its addresses answer |

Each has a `README.md` beside it saying what it owns, what it emits, what it
keeps and for how long — and what it deliberately does not do.

## The kernel

| Module | What it is |
|---|---|
| `db` | the pool, `TenantConn`, and the loudly named `OperatorConn` |
| `tenant` | `TenantId`, and resolving an address to a site |
| `authz` | the Cedar engine, its schema, its policies, and `Permit` |
| `http` | `Guard`, `Endpoint`, the router, and who is asking |
| `error` | one error shape, one list of codes |
| `audit` | `Auditable`, and the receipt a mutation cannot answer without |
| `events` | `EmitsEvents`: an outbox row in the transaction that made the change |
| `queue` | typed work, claimed with `skip locked` under a lease |
| `scheduler` | a day's work, once, under one lock for the cluster |
| `ratelimit` | a window counted in Postgres rather than in one process |
| `retention` | what each table keeps, for how long, and what takes it away |
| `openapi` | the description, generated from the list of endpoints |
| `metrics` | what to scrape |
| `page` | one pagination shape, cursor-based |
| `say` | what somebody is told, as a key rather than a sentence |
| `browser` | what a browser is told, and what is believed when one asks |
| `domain` | what this is made of, as a list rather than a habit |
| `outbound` | reaching an address somebody else configured, safely |
| `typescript` | the panel's types, written from the description |
| `mailer`, `payments`, `builder`, `transcoder`, `storage` | who does the work that is not this process's |
| `money`, `secret`, `clock`, `types` | the types that keep a class of mistake out |

## What is checked rather than remembered

| | Where |
|---|---|
| Every tenant table forces row-level security | `tests/schema.rs` |
| Every foreign key has an index leading with it | `tests/schema.rs` |
| Uniqueness is a site's own, not the machine's | `tests/schema.rs` |
| Every table holding somebody's data says when it goes | `tests/schema.rs` |
| What a retention policy names as its sweep exists | `tests/schema.rs` |
| The answers the engine gives, in full | `tests/snapshots/permission-matrix.txt` |
| What the API is | `tests/snapshots/openapi.json` |
| What is served without an account | `tests/http.rs` |
| Nothing public is without a rate limit | `tests/http.rs` |
| Nothing behind an account answers without one | `tests/http.rs` |
| A change that records nothing cannot answer | `tests/http.rs` |
| A listing costs the same for ten rows as for one | `tests/forms.rs` |

## Working on it

Read `DOMAIN_TEMPLATE.md` before writing a domain, and fill one in. The
cross-cutting list at the bottom of it is what a domain PR is reviewed against.

## What is not here

Written down rather than left to be discovered.

- **Nothing builds a site.** Publishing does everything around a build — one at
  a time, recorded, counted — and the generator itself is a container this
  repository does not hold.
- **This crate cannot make a second site.** `/api/setup` makes the one tenant,
  its operator and its owner account, in one transaction, and answers once;
  nothing else here ever inserts a `tenants` row. The isolation machinery under
  that — `tenant_id`, row-level security, a request resolved from `Host` — is
  still here today and is coming out rather than staying
  ([#4](https://github.com/productdevbook/mavi/issues/4)), but running many
  installations from one machine has never been a mode this crate has, before
  or after that lands. That is a hosting product built on top of this, through
  `kernel::outside`, not a mode of this crate.
- **No billing, metering or plan of any kind.**
- **No console over more than one installation.** The machine's own screens are
  this one site's, not a fleet's.
- **`transfers` has no code behind it.** Everything else once named here as
  unwritten — `plugins`, `assistant`, the import half of `portable` — has
  endpoints now.
- **No data migration from v0.** Nothing here reads the schema-per-tenant
  database beside it; that script belongs with whichever domain moves first.
