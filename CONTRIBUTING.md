# Contributing to Mavi

Changes are welcome, including ones that say the way something is done here is
wrong. What follows is what a change is expected to come with, so that a
review is about the idea rather than about the mechanics.

## Running the tests

The backend needs a Postgres. Any will do:

    docker run -d --name mavi-test-db -p 127.0.0.1:5433:5432 \
      -e POSTGRES_PASSWORD=test -e POSTGRES_DB=mavi_test postgres:18-alpine
    export TEST_DATABASE_URL=postgres://postgres:test@127.0.0.1:5433/mavi_test

    cd server
    cargo fmt
    cargo clippy --all-targets --all-features -- -D warnings
    cargo nextest run --workspace

Every test that wants a database makes one of its own and migrates it. There
are sixty-seven of them, and one of them is every migration in the schema — a
check constraint nothing ever ran is a claim rather than a rule.

`old/` is what still runs the sites while `server/` is being finished. Its own
three commands are the same, in `old/`, with `--profile ci`. Read
`old/README.md` before changing anything in there.

The panel:

    cd client
    bun install
    bun run build && bun run typecheck && bun run lint

The build is what generates the route tree, so it comes before the typecheck —
`tsc --noEmit` on its own checks nothing here.

After touching any string somebody reads, `bun run extract` and translate the
new ones. The panel ships English and Turkish, and half a translated screen is
worse than none.

## What the tests already refuse to let you do

Some of what would normally be review comments is a failing test instead:

- An endpoint that lists things and does not answer a page.
- A shape the API names twice, meaning two different things.
- A table with a site's data on it and no policy hiding it from other sites.
- A foreign key with no index to read it by.
- A job kind nothing claims, or a claim for a kind nothing declares.
- A panel type that no longer matches what the API says it answers.

If one of these fails, it has found something. Read it before changing it.

## What a change comes with

- A test that fails without it. Names read as sentences; one behaviour per test.
- A commit message in prose saying **why**. What changed is in the diff.
- Comments only where the code cannot speak: a constraint that reads as wrong,
  an outside behaviour nobody would guess, the reason a choice was made. Not a
  restatement of the line below, and not a changelog.

## Two rules about what goes in

**Nothing about anybody running it.** This repository is public and permanent.
No address, no name, no hostname, nothing out of a database somebody is using,
no credential — in code, in a test, or in a commit message. Test data uses
names that are obviously invented.

**Licences are checked rather than assumed.** MIT is what this is, and it
decides what may be brought in. `LICENSES.md` is the record of having looked.
