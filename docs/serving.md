# What a visitor sees

One installation is one site, and one process answers both halves of it: the
panel's API, and the site's own pages. There is no second deployment, no
container in front, and nothing to add when a page is published — a page
appears because a row changed.

## The two halves on one address

Everything under `/api` is the API and answers as the API all the way down: a
path nothing describes gets the same refusal shape as every other refusal, so
a client that mistyped something never receives a page of HTML it cannot
parse. Everything else is the site.

That is one line in `mavi-everything/src/mounted.rs`, in the router's
fallback, and the test that keeps it there is in
`mavi-everything/tests/what_a_visitor_sees.rs`.

## A build is a folder, going live is a row

Somebody working on how a site looks is working in a **set of changes**. When
they ask for it to be built, what comes out is written under a name that is
that set's own id:

    builds/<the set of changes>/index.html
    builds/<the set of changes>/about/index.html
    builds/<the set of changes>/styles/site.css

Nothing moves when it goes live. `changes.at = 'published'` is what says which
folder is served, and it is one statement that publishes one and puts the
previous one back — so there is never a moment with two published sets or
none, and going back is that statement again with the older id.

There is deliberately **no job that puts a build live**. A row already answers
which build is served; a job that ran afterwards to make it true would be a
second answer to the same question, and the gap between them is a site serving
neither.

## What is built

**How** a site is built is a port — `Builds` — for one reason: a design that
has to be built is a project with its own dependencies and its own command,
and running it is a machine running whatever somebody else wrote. That is a
sandbox, a scheduler and a quota rather than a function, and none of them
belong in a library anybody installs.

So this software says what it needs and does not say how:

> given everything in this set of changes, what should a visitor be served

What ships with it is `WhatIsInPublic`: whatever a design put under `public/`
is the site, as it is. A site of plain files is a real site rather than a
degenerate case, and nothing runs — so there is nothing to sandbox and nothing
to wait for.

`src/` is what a generator would read, and to something that does not run one
it is not a page. Serving somebody's templates would be publishing the thing
that makes the pages.

A host that builds each site's own project hands in its own implementation
instead, and nothing above the port knows which one it got.

A build that failed is **not an error.** A design that does not compile is an
ordinary thing for somebody to go and fix, and what they need is the message —
so it comes back as an answer and is written down where the panel shows it. An
error from the port means the builder itself could not be reached, which is
this end's problem rather than theirs.

## Which file answers an address

- A folder is its index. `/about` and `/about/` are both `about/index.html`.
- Anything with a dot in it is asked for by name.
- Nothing that climbs is a page. `..`, a backslash, and the encoded forms of
  both are refused before a store is asked, because what `/../../etc/passwd`
  should get is the site's own "not here" rather than an answer that says
  which guard stopped it.

What a browser is told a file is comes from a list, from the name, in one
case: `LOGO.PNG` is a picture. Never from what is inside the file — a kind
guessed from contents is a file a browser can be talked into running.

## When something is not there

Four different answers, and the difference matters:

| What happened | What a visitor gets |
|---|---|
| Nothing has ever been published | `404`, plainly. An installation on its first day is not an error. |
| The database is not answering | `503`. A four-oh-four is a fact about the site that caches and search engines act on, and saying it during an outage is how a site is quietly deindexed. |
| A page is not there | The site's own `404.html`, if it built one. |
| A **stylesheet** is not there | `404`, plainly. Answering a missing `.css` with a page of HTML is how a stylesheet becomes a parse error in somebody's console. |

## A page that moved

Renaming a writing writes where its old name now points, **in the same
transaction as the rename**. A redirect written afterwards is one that a crash
between the two loses, and a rename with no redirect is every link anybody
ever made answering "not here".

What is kept is the name, not the whole address, because nothing in this
software knows where a design puts its posts. `/blog/old` → `/blog/new` and
`/writing/old` → `/writing/new` are the same row.

One name used in two languages is not guessed between: the address says which
only when the design writes the language into it, and sending half the readers
to a page in a language they did not ask for is worse than not sending them.

## Looking at something not published

A set of changes that has been built answers under its own id:

    /_looking/<the set of changes>/

Nothing links there and nobody guesses a uuid. The id is the whole of the
secret and it is enough of one — an id that is not a build finds no files, so
there is nothing further to ask.

## Caching

A minute, and an `ETag` of the build and the size of what is going back. An
address does not carry which build answered it, so a longer hold would serve
yesterday's page out of somebody's browser after a publish. The fix for that
is names with a fingerprint in them, written by whatever builds the site —
not a bigger number here.

The tag is read as well as written. A request carrying `If-None-Match` for
the page it already has is answered `304` with nothing in it — a list and
`*` both understood, because both are the specification's and neither is
what a browser usually sends, which is why they are the two that go
untested.

Whatever a build produced is the site's own, and nothing in the way decides
what it may do: a content policy this software put on its own answers would
break a page that loads its own stylesheet.

## Where it is

| File | What it decides |
|---|---|
| `server/mavi-edge/` | which file answers an address, what it is, where a page went — no database, no store, no HTTP |
| `server/mavi-everything/src/building.rs` | turning a set of changes into files |
| `server/mavi-everything/src/showing.rs` | where the bytes come from and what a browser is told |
