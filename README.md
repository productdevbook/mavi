# Mavi

A content management system you run yourself. One Rust binary, one PostgreSQL
and a React panel: a site with pages and posts, that sells things, teaches
courses, takes what people type into forms and sends mail about all of it.

One installation is one site — see [why](#one-installation-one-site) — and it
is a CMS, not a hosting business. What running many sites on one machine needs
on top of this — metering, billing, a console over many of them — is
deliberately not here; see [what this is not](#what-this-is-not).

MIT. Run it, change it, sell it.

- **One binary, one database** — Axum and sqlx over PostgreSQL. Migrations run
  at boot, and the tests run against a real Postgres rather than a substitute.
- **One site** — a request is resolved from its `Host` header, and everything
  it can reach belongs to that site.
- **[Whatever the site publishes](#more-than-posts)** — posts and pages, and
  any kind of thing a site makes up: a course with a price and a level, a
  property with rooms. Each carries its own fields beside the title and the
  body.
- **Multilingual** — a site says which languages it writes in, and the same
  writing in two of them is one group rather than two unrelated posts.
- **[Publishing](#publishing)** — a design is written to a draft, built
  somewhere to look at, and put live when somebody says so. A post given a date
  goes out on it, within the minute.
- **[Assistants](#assistants)** — every site answers the Model Context
  Protocol. Point an assistant at it and ask it to do the work.
- **[Teaching](#teaching)** — courses, modules, lessons and videos, with access
  that can be sold for ninety days and actually ends after ninety days.
- **Selling** — products, stock held at checkout, discount codes with
  conditions, and orders numbered per site.
- **Everything is written down** — every change writes an audit row before it
  can answer, and the log can be read, filtered and taken away as a file.

## Quick start

```bash
curl -O https://raw.githubusercontent.com/productdevbook/mavi/main/docker-compose.yml
curl -O https://raw.githubusercontent.com/productdevbook/mavi/main/Caddyfile
{
  echo "POSTGRES_PASSWORD=$(openssl rand -base64 24)"
  echo "MAVI_KEYS=1:$(openssl rand -base64 32)"
  echo "MAVI_URL=http://localhost"
} > .env
docker compose up -d
```

No line is optional and none has a default. The database holds every site on
the machine, and `MAVI_KEYS` is what seals every secret a site keeps — its mail
password, its payment keys. A key that ships with the software is one everybody
else running it also has; a key that changes on restart is a site whose secrets
can no longer be read.

`MAVI_URL` is the address this answers on, as somebody outside would type it,
and it is what a link in a letter is built from. It has no default because the
letter that needs it most — the one somebody clicks to choose a password — is
sent by a scheduled job, which has no request to take an address off; a guess
here would send everybody a link that works on the machine that sent it and
nowhere else.

Open <http://localhost> and set up the first account. That makes the site too
— its address is whatever you reached the machine on — and signs that account
into it. That is the whole of setup: where the database is was decided before
the process started, and there is nothing after this to make a site with.

On a machine other people can reach, give it your own name instead — put
`MAVI_DOMAIN=example.com` and `MAVI_URL=https://example.com` in `.env`, point
the name at the machine, and Caddy asks for a certificate on the first request. Anything else can stand in front
instead: nginx, Traefik, whatever is already there. All this needs from it is
the `Host` header passed through and `X-Forwarded-For` and `X-Forwarded-Proto`
set — how often somebody may try a password, and what the record says a change
was made from, are decided from the address they arrive on.

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
| Panel | `ghcr.io/productdevbook/mavi-panel` |

Both are built for `linux/amd64` and `linux/arm64`.

### Configuration

The API reads these; everything else is set from the panel.

| Variable | Default | Notes |
|---|---|---|
| `DATABASE_URL` | — | PostgreSQL. Required. |
| `MAVI_KEYS` | — | What seals a site's secrets. `1:<thirty-two bytes, base64>`, and a version and comma for each older key. Required; the process refuses to start without it, and refuses to start on one it cannot read rather than making one up. |
| `MAVI_URL` | — | The address this answers on, as somebody outside would type it: `https://example.com`, or with the path it is served under. What a link in a letter is built from. Required; the process refuses to start without it, because a letter with an unusable link in it is a person who never got back into their account. |
| `MAVI_ROLE` | `both` | `api`, `worker`, or `both`. One process can do both; two make the queue somebody else's problem when the API is busy. |
| `MAVI_DATA_DIR` | `uploads` beside the process, and `/data` in the image | Uploaded media. **Must be a persistent volume**, or everything anybody uploads goes with the container. |
| `HOST` / `PORT` | `0.0.0.0` / `8080` | |
| `GENERATOR` | — | A command run in a workspace holding that site's `src/` and `public/` and nothing else. It brings its own project: what decides how a site is built cannot be written through the API. Unset, what the theme put in `public/` is served as it is. |
| `GENERATOR_OUTPUT` | `dist` | Which directory that leaves the built site in. |
| `RUST_LOG` | `info` | |

The panel is static files behind nginx, which proxies `/api`, `/mcp`,
`/uploads` and `/openapi.json` to the API.

## One installation, one site

Setup makes exactly one site, and there is no way to make a second: `/api/setup`
answers once, and nothing else in this crate ever inserts a `tenants` row.
That is not a limitation left to be lifted later — it is what this is. Running
many sites on one machine is a hosting product built on top of this, not this.

Today, that is still built on isolation: a `tenant_id` on every table that
holds a site's data, row-level security **enabled and forced** on every one of
them, and a request resolved from its `Host` header rather than trusted to say
which site it is. A connection is opened with the site it belongs to and the
database refuses to hand it anything else, whatever a query says — nothing has
to remember to filter by tenant, and the one test that matters is a schema
test: a table with a `tenant_id` and no policy on it fails the build.

That machinery is coming out rather than staying —
[issue #4](https://github.com/productdevbook/mavi/issues/4) tracks removing
it. Machinery built for hosting many sites and used for one still has to be
understood by everybody reading the code and kept correct by everybody
changing it, in exchange for a capability this project does not offer.

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

Requires [Bun](https://bun.sh) and a Rust toolchain.

```bash
bun install
bun run dev          # http://localhost:5173, proxies the API to :8080

cd server
cargo run            # http://localhost:8080
```

The panel expects the API on `:8080`; point it elsewhere with
`VITE_API_PROXY_TARGET`.

```bash
bun run build        # builds, then typechecks — vite generates the route tree
bun run typecheck
bun run lint
bun run extract      # pull new translatable strings into src/locales/*/messages.po

cd server
cargo clippy --all-targets -- -D warnings
cargo nextest run --profile ci
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
src/                 React 19, Vite, TanStack Router, Tailwind 4, Tiptap 3
server/src/kernel/       what every domain is built out of: the router, the
                     database, authorization, the queue, the words a refusal
                     is said in
server/src/<domain>/     one folder per thing a site does — content, media, mail,
                     shop, learning, flows, publishing, people
server/migrations/       the schema, run at boot
server/types/            the panel's types, generated from the API's description
wordpress-plugin/    the WordPress migration plugin (GPLv2+)
```

The panel is English and Turkish, via [Lingui](https://lingui.dev). Which
language the panel is read in has nothing to do with which languages the site
writes in.

## The parts that need more than a paragraph

| | |
|---|---|
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
`server/src/kernel/outside.rs` lets a crate that depends on this one hand in
its own endpoints and its own kinds of queued work, mounted through the same
guard, the same rate limit and the same audit rule as everything here. Nothing
mounted that way can skip a permission check or a receipt, and a test says so.

It is also not a plugin marketplace. What a site can be made to talk to — its
mail server, its payment provider — is a decision in the software rather than
a form somebody fills in, and adding a third is a change to this repository.
