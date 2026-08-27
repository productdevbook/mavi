# Working on Mavi

This repository is public. Everything in it — code, comments, tests, commit
messages, documentation — is readable by anyone, forever, including by search
engines and by whoever forks it.

## Nothing about anybody running it

The rule that matters most, and one that has been broken before: a commit
message and a doc comment once carried two real customers' email addresses
into public history.

Never write, in code or in a commit message:

- **Email addresses, names, or company names** of anyone using this software.
- **Hostnames** of anybody's installation.
- **Anything out of a database somebody is using** — post titles, categories,
  user names.
- **Credentials of any kind**, including ones that have since been changed. A
  rotated password still says how somebody chooses passwords.
- **Server addresses, cluster details or bucket names** belonging to whoever
  is running it.

Something that happened while running an installation is often the reason a
fix exists, and that reason is worth writing down. Write the *shape* of it:

> An agency whose address matches an editor already on the site would have
> taken that account over.

not

> The agency is a@example.com and so is the editor.

The same goes for test data: names that are obviously invented.

## Where things are

    server/       the API — a Rust workspace, one crate per thing a site does
    server/mavi-core/       the vocabulary every other crate is built out of
    server/mavi-storage/    the pool, the site-scoped transaction, and
                            migrations — one file per change, applied at boot
    server/mavi-contract/   the canonical catalog every artifact is generated
                            from: OpenAPI, the TypeScript and Rust clients, MCP
    server/mavi-http/       every route, and the questions no one crate can ask
                            about itself
    client/       the panel — React, TanStack Router, Lingui (English, Turkish)
    docs/         one document per thing that is not obvious from the code

A domain crate depends on the foundation and not on another domain. There is
no cycle in the workspace, and adding one is the kind of change to stop and
ask about.

One installation is one site: `/api/setup` makes an owner role and the account
able to sign into it, in one transaction, and answers once.

There is no tenancy in the sense that decides things: no `tenants` table, no
`tenant_id`, and in the self-hosted runtime nothing that resolves a request to
one of several sites — a request arrives at this installation because this
installation is what it reached. A database holding more than one site is
**refused** rather than served, on every request, because a query that picks
one of several silently is worse than one that stops.

Every table a site owns still carries `site_id`, under row-level security that
is **forced**, so the policy holds against the owning role too. The scope is
set inside the transaction, never on the pooled connection:

    select set_config('app.site_id', $1, true)

That is the belt around `fixed_site`'s braces: each process owns one configured
site, while any multi-site hosting product stays outside this repository.

Running many sites on one machine is a hosting product built on top of this,
not a mode inside it.

`mavi-core` is what every other crate is built out of: the identifiers, the
errors, cursor pages, and the site vocabulary. `mavi-storage` owns the pool and
the site-scoped transaction, `mavi-audit` the receipt, and Hatchet owns durable
workflow delivery through the Rust outbox and Go bridge.

What a hosting business needs — metering, billing, a console over many sites —
is built on this rather than in it, and lives in its own repository.

## Before every commit

    cd server
    cargo fmt
    cargo clippy --all-targets --all-features -- -D warnings
    cargo nextest run --workspace

Some tests want a Postgres, because a site is rows in one. They are
`#[ignore]`d and read `TEST_DATABASE_URL`, so the three commands above skip
them until one exists — and one of them is every migration in the schema:

    docker run -d --name mavi-test-db -p 127.0.0.1:5433:5432 \
      -e POSTGRES_PASSWORD=test -e POSTGRES_DB=mavi_test postgres:18-alpine
    export TEST_DATABASE_URL=postgres://postgres:test@127.0.0.1:5433/mavi_test

Every test gets a machine of its own. An installation is one site, so two
tests cannot share a database and still be two installations — but migrating
one per test would run every migration three hundred times. So a few databases
are kept and leased: a test holds one for as long as its process lives and is
handed it emptied of whatever the last holder left.

The panel:

    cd client
    bun run build && bun run typecheck && bun run lint

The build is what generates the route tree, so it comes first — `tsc --noEmit`
alone checks nothing here. After touching any string somebody reads, `bun run
extract` and translate the new ones: a half-translated screen is worse than an
untranslated one.

## How to work here

Measure before saying. Nearly everything here that turned out to be broken
looked fine in the code and was only visible by running it: a form with no
limit on it, a queue two workers could take the same row from, a JSON null
SQLite accepted and Postgres refused, a select that showed a uuid where a
name belonged.

Comments explain what code cannot: a constraint that reads as wrong, an
outside behaviour nobody would guess, the reason a choice was made. Not what
the line does, not what changed, not a changelog.

Commit messages say why, in prose. What changed is in the diff.
