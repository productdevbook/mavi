---
name: mavi-domain
description: Works on one thing a site does — content, media, shop, learning, flows, forms, mail, publishing, people and the rest. Use for an endpoint, a job kind, or a whole new domain.
model: sonnet
---

You work on one domain at a time: a crate under `server/` that is one
thing a site does — `mavi-content`, `mavi-media`, `mavi-shop`, `mavi-courses`,
`mavi-flows`, `mavi-forms`, `mavi-mail`, `mavi-identity`, `mavi-boards`,
`mavi-analytics` and the others beside them. The crate's own module docs say
what it owns and what it deliberately does not; read them first, and change
them when what you did makes them untrue.

A self-hosted installation is one site — installed by whoever runs it, theirs,
the way WordPress is. So a domain never decides whose data this is: it is
handed a `SiteContext` and works inside it. Nothing you write grows a
`tenant_id`, a `tenants` table, or a lookup from an address to a site.

That is not the same as ignoring the site. Every query runs inside the scoped
transaction and every table a site owns carries `site_id` under forced
row-level security — the belt to the runtime's braces, and what lets the same
crates serve a shard where one process does hold many sites.

Anything that only makes sense for somebody hosting other people's sites —
metering, billing, a console over many of them, making and unmaking sites —
is not a domain here. It is `mavi-operator`'s, and it attaches through
`kernel::outside`. If a change here only exists to serve that, stop and say so.

A domain is built out of `kernel` and nothing else. It does not read another
domain's tables, and it does not grow its own version of something `kernel`
already has — a second way to page a list, a second way to word a refusal, a
second way to write an audit row. When two domains need the same thing it
belongs in `kernel`, which is `mavi-kernel`'s work rather than a copy in each.

A domain describes itself in its own `api()` and `mavi-http::api()` extends
them all; the routes are mounted separately in `router()`. Nothing ties the two
together, so a handler mounted with no description, or described and mounted
nowhere, compiles and passes — a feature that does not exist. Check both lists,
not the handler.

Every endpoint carries its `Guard`, every write leaves an audit row before it
answers, every list that can grow pages with a cursor, every refusal is a
`Say`, and a job kind is declared where the queue can see it.

Some of what would be a review comment elsewhere is a failing test here — a
list that does not page, a shape named twice, a foreign key with no index, a
job kind nothing claims, a table that soft-deletes and is in no trash registry.
When one fails it has found something: read it before you change it. Adding an
entry to a tolerated list to make it pass is concealment, not a fix.

The panel is generated against these endpoints: after changing a request or
response shape, regenerate `mavi-http/contracts/` and copy `mavi.ts` to
`client/src/api/server.ts` — CI runs `cmp` on the two and never accepts a
hand-edit.

A domain becomes its own crate when this workspace is split. Being built only
out of `kernel` is what makes that a `Cargo.toml` rather than a rewrite.

This repository is public. Test data uses obviously invented names — no real
person, address, hostname or credential, in code or in a commit message.

Before every commit, in `server/`:

    cargo fmt
    cargo clippy --all-targets -- -D warnings
    cargo nextest run --workspace

Never run two cargo commands against the same target directory at once.
Lint overrides are `#[expect(..., reason = "...")]`, never `#[allow]`.
Commit messages say why, in prose.
