# Goal

Everything a site does, in one Rust binary and one Postgres, installed by
whoever runs it.

The measure is concrete: somebody moving a site off WordPress should not find a
hole where something they relied on used to be, and should not have to reach
for a plugin directory to fill it. WordPress is the yardstick because it is
what most self-hosted sites actually are — and because "you install it and it
is yours" is the property being copied, not the codebase.

What is **not** answered is listed here too, with the reason, so a gap is a
decision somebody can find rather than something forgotten.

## One installation is one site

Not a limitation waiting to be lifted. `/api/setup` makes the account and the
site in one transaction and answers once, and nothing else in this crate ever
makes a second.

Running many sites for money — provisioning, addresses and certificates,
metering, billing, a console over all of them — is a different product built
on this rather than a mode inside it. Every trace of it is being removed, and
what that removal is doing is tracked in #4.

## What a host provides instead

This is a binary somebody runs, and it is also a library something can embed.
What a host already solves is asked for rather than built:

| Not built | Asked for as | Why |
|---|---|---|
| where the database is, what seals its secrets | `Config` | read at the edge, handed in at construction — a library that reads `std::env` in its constructor makes every consumer inherit a global |
| which address this installation answers on | `Config` | one configured value; a scheduled job sending a letter has no request to take one from |
| where mail actually goes | `Mailer` | this crate says a letter should be sent; a host's provider sends it |
| who takes the money | `Payments` | a shop is not a payment gateway |
| what turns a video into something a browser plays | `Transcoder` | |
| what builds a site's own pages | `Builder` | what decides how a site is built cannot be written through the API |
| certificates, DNS, load balancing, backups | — | the host, the shell, the operating system |

The direction that matters: **this crate asks, it does not receive.** A seam
that lets something outside hand in endpoints and job kinds existed and is
being removed (#13), because it had one user and that user needed the opposite
— a library it could construct, not a host it could extend.

## Where this is

Measured, not estimated:

| | |
|---|---|
| The API | **177 operations** across 121 paths, 179 named schemas, all described in a snapshotted OpenAPI document |
| The crate | 29,196 lines of Rust in 82 files, 28 modules, one crate |
| The schema | 47 migrations, applied at boot |
| Tests | 42 files against a real Postgres, each leasing a database of its own |
| The panel | 42 routes, React and TanStack Router, English and Turkish with nothing missing |

### It is being rewritten, in `crates/`, one crate at a time

The numbers above are `server/`, which is what runs today and will keep running
until the new tree can serve. Beside it, `crates/` is the same software written
again as a workspace — the owner's decision, and #10 carries it.

Where that has got to, counted the same way:

| | |
|---|---|
| The API | **101 operations** across 70 paths, every one declaring its parameters, its failures, the status it answers and how to authenticate |
| Reachable | **all 101 answer**, through the guard and the audit rule, against a real Postgres |
| The workspace | 22 crates — eight of foundation, twelve domains, one that holds the whole API and asks it what no domain can ask about itself, and one that puts files somewhere |
| The schema | 16 migrations, applied to a real Postgres by a test rather than believed |
| Tests | over 330, of which about 60 need a database and get one of their own |
| The panel | not started, on purpose: it is written last, against an API worth writing one against |

That second row is a rule rather than a count: `Site::not_reachable` must be
empty, so an endpoint described and mounted nowhere fails the build. A
description with no route is a feature that does not exist.

`server/` is still what runs, and moving to the new tree is its own decision
with its own migration of data behind it. What is left before that is worth
naming: the worker loop that takes the queue's work, the scheduler that queues
anything on a timer, and the panel.

What that is *for* is worth stating, because "a rewrite" on its own is a bad
reason. Every crate down there exists to make one measured failure
unrepeatable:

| Crate | What it makes impossible |
|---|---|
| `mavi-core` | A refusal that can only be said in English. A listing whose cursor addresses less than its order — the failure that hit fourteen listings and skipped rows silently. Money compared across currencies, or multiplied into a number that saturated. An id passed where another belongs. An address that is two accounts because one of them capitalised it. |
| `mavi-db` | The `order by` and the cursor predicate disagreeing, because both are generated from one declaration. A migration nobody has ever run: every one of them is applied to a real Postgres in CI, and the constraints in them are asserted by the thing they refuse. |
| `mavi-api` | An endpoint that does not say its parameters, its failures, its real status, or how to authenticate — all four of which were missing from all 177 operations. Two endpoints that are secretly one route. |
| `mavi-http` | A change that answers without a record, decided by what the endpoint said rather than by the HTTP verb. Two admission paths, one of which had no audit gate. |
| `mavi-audit` | A receipt for a change that was never written: `record` is the only thing that makes one, in the change's own transaction. A record anybody can rewrite, refused by the database rather than by the code. |
| `mavi-content` | An address in use being taken by a check-then-write race. Published and its date disagreeing. |
| `mavi-taxonomy` | A tag with a parent; a category under a tag; a term under itself. |
| `mavi-media` | A file kept under the name somebody chose. A script called `holiday.png` served back as an image. |
| `mavi-forms` | A form that declares nothing accepting anything — which is what it did. A submission bounded per answer and unbounded in total. A public endpoint mixed in with the panel's. |
| `mavi-mail` | Somebody who left the newsletter being unable to reset their own password. A campaign with no way out of it. A letter that went out with `{link}` still in it. |
| `mavi-shop` | An order sent that was never paid for — which the old schema's own constraint permitted. Two checkouts deadlocking over the same two products. A discount that takes the total below zero, or comes off in the wrong currency. |
| `mavi-courses` | A student opening a lesson in a course that was closed. A reorder that leaves a module at minus one. |
| `mavi-flows` | A flow calling the database, the metadata service, or the machine next to it. A step that cannot run, written and only discovered once per order. |
| `mavi-design` | Anything that decides how a site is built being written through the API. A layout going live without being built and looked at first. |
| `mavi-boards` | Two cards in one place, after the fiftieth time somebody dropped one between the same two. |
| `mavi-work` | Two workers running one job. A worker whose lease lapsed marking done a job somebody else now holds — which put the row back to `ready` and ran the work a third time. Work queued for a kind nothing runs. |
| `mavi-serve` | A route nobody described. An endpoint that is described and mounted nowhere going unnoticed — that is a number this crate answers. A refusal shaped one way for endpoints and another for the parts of a router nobody wrote. |
| `mavi-everything` | Two crates describing one route, or naming one endpoint twice. A capability nothing asks for, or one asked for that a site cannot grant. |

The dependency graph made this cheap rather than heroic: **22 of the 27 domains
already depend on nothing but the kernel**, and cutting the kernel's six
outbound edges leaves no cycles at all. Those six are cut (#76). The lines were
already where they needed to be; the workspace is what makes the compiler hold
them.

The order is the foundation first, then the domains, then the panel — rewritten
last, from nothing, once there is an API worth writing one against.

### What is not finished, named rather than implied

The API is described but not yet worth generating a client from: **no operation
describes a parameter, a failure, or how to authenticate**, and 67 of the 177
answer a status they do not declare. That is #11, and it is the largest single
gap between what this is and what it claims.

Twelve listings cursor on less than they order by (#62). The first screen the
panel opens counts eight whole tables every time (#60). Finished jobs are never
swept (#61). The audit gate decides what a change is by the HTTP verb, which is
wrong for any endpoint carrying a protocol (#54).

Everything else open is in the tracker, measured, with the case in which it
bites written down.

## What a site does

Each of these is a thing WordPress does that somebody would miss.

- [x] **Posts and pages** — and any kind of thing a site makes up, with its own
      fields beside the title and the body. A course with a price and a level,
      a property with rooms.
- [x] **Categories and tags** — one taxonomy, applied to any kind.
- [x] **Media** — uploads, images, video with transcoding.
- [x] **Menus, themes, layouts** — a site's own project, built and published;
      what decides how it is built is deliberately not writable through the API.
- [x] **Multilingual** — a site says which languages it writes in, and the same
      writing in two of them is one group rather than two unrelated posts.
- [x] **Forms** — and what has come in through them, with retention.
- [x] **Mail** — campaigns, lists, transactional letters a site words itself.
- [x] **Users and roles** — grants, an owner role that cannot be stranded, a
      second factor, OAuth.
- [x] **Scheduled publishing** — a post given a date goes out on it, within the
      minute.
- [x] **Redirects** — a renamed slug keeps its old address working, followed at
      the edge rather than by the theme.
- [x] **An audit log** — every change writes a row before it can answer.
- [x] **Selling** — products, stock held at checkout, discount codes, orders.
- [x] **Teaching** — courses, modules, lessons, video, enrolments that end.
- [x] **Assistants** — the whole site answers the Model Context Protocol.
- [x] **Flows** — what happens when something happens.
- [x] **Export and import** — a site can leave.

### Not built, and why

| | |
|---|---|
| **Full-text search** | There is none. A listing filters — by kind, by a declared field, by language, by category or tag — and every one of those is exact. WordPress answers `?s=`; this answers nothing. **This is the largest missing thing on the list** and it was found while writing this document, by checking a claim rather than repeating it. |
| **Comments** | Nobody has asked for them, and a comment system is a spam system with a comment feature attached. It is a real gap for anybody moving a blog; it is not being pretended away. |
| **A plugin directory** | Running somebody else's code inside this process is a decision, not a feature. `plugins` configures providers; it does not load code. |
| **Themes as a marketplace** | A site's project is a directory somebody builds. There is no store, no licensing, no updater. |
| **Multisite** | See above. It is a different product. |
| **A visual page builder** | The editor writes documents. Laying out a page is the theme's job. |

## How this is worked on

Nothing is built or tested on the machine this is written on — it serves other
people's sites, and a build taking every core has taken it off the air before.
The loop is: branch, write, commit, open the pull request, read what CI says.

Every rule this repository holds has a test behind it rather than a habit: that
every reachable endpoint is described, that nothing public is without a limit,
that every change writes an audit row, that a paged listing cursors on what it
orders by, that a letter kind a site can word is one something presses, that
every retention policy names a real job, that everything soft-deleted is in the
trash registry.

When one of those goes red, the code is wrong or a reason is missing. Adding a
name to a tolerated list, relaxing an assertion, or deleting a test is
concealment rather than repair.
