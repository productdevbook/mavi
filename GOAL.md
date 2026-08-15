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

**One crate, not a workspace.** The dependency graph was measured: 22 of the 27
domains depend on nothing but `kernel`, and cutting the kernel's six outbound
edges leaves a graph with no cycles at all. So the boundary a workspace would
enforce is one the code already keeps, and splitting would buy compile-time
enforcement of a rule nothing is currently breaking, at the cost of 42 test
binaries each linking every member. The measurement is in #10 and the issue
says why it stays shut.

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
