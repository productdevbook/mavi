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

    server/       the API, the queue and the scheduler — one Rust crate
    server/src/*/   one module per thing a site does, each with its own README
    server/migrations one file per change, applied at startup
    src/          the panel — React, TanStack Router, Lingui (English, Turkish)
    docs/         one document per thing that is not obvious from the code

`server/src/kernel/` is what every module is built out of: the guard on an
endpoint, the audit receipt, the queue, cursor pages, and `Say` — a refusal is
a key with named arguments so it can be said in somebody's own language.

`server/src/kernel/outside.rs` is the seam something outside this crate attaches
through: endpoints and job kinds, mounted through the same guard and the same
audit rule as this crate's own. It exists because what a hosting business
needs — metering, billing, a console over many sites — is built on this rather
than in it.

## Before every commit

    cd server
    cargo fmt
    cargo clippy --all-targets -- -D warnings
    cargo nextest run --profile ci

Some tests want a Postgres, because a site is rows in one:

    docker run -d --name mavi-test-db -p 127.0.0.1:5433:5432 \
      -e POSTGRES_PASSWORD=test -e POSTGRES_DB=mavi_test postgres:18-alpine
    export TEST_DATABASE_URL=postgres://postgres:test@127.0.0.1:5433/mavi_test

No test migrates its own database. Each shape is migrated once into a template
by whichever test asks for it first, and every test after that is handed a copy.

The panel:

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
