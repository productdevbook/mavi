# What this software asks a host for

Five things, and every one of them is something the host already has or
already has an opinion about:

| | |
|---|---|
| `Clock` | what time it is |
| `Files` | where an uploaded file goes |
| `Builds` | turning a design into what a visitor is served |
| `Mailer` | where a letter goes |
| `Told` | what happened, said outward |

A port is a decision, not a convenience. `server-next/mavi-core/src/ports.rs` says
so at the top: adding one is work for everybody embedding this, and one nobody
implements differently is a parameter wearing a costume.

## Why there is no plugins table

The software this replaces had one: 524 lines where a site chose its own mail
server, its own payment provider, and where those settings were kept in rows,
tested for whether they were "working", and fallen back from to a machine-wide
default when a site had configured nothing.

Every one of those is a port, done worse:

- **The credentials live in the database.** A port takes a configured thing
  that the host built; a table takes somebody's SMTP password and keeps it
  where a backup, a query and a support session can all reach it.
- **The list of what can be plugged in is compiled in anyway.** The old
  module's own comment says it: "which integrations exist is the list below
  rather than whatever somebody writes into a table". So it was never
  extensible — it was a port with a settings screen in front of it.
- **"Working" is a column.** Whether mail is going out is not a fact about a
  row, it is a fact about the last time something was sent, and a column
  saying `true` is a column that was true once.
- **The fallback hides the question.** A site with nothing configured used the
  machine's own server. That is exactly what a host providing `Post` *is* —
  written twice, with a branch between the two copies.

So: **a host that wants a site to send through its own mail server implements
`Mailer` that way.** What this software knows is that a letter should be sent
and what it says. How mail leaves a machine is not its business, and a host
that already sends mail should not gain a second way to.

The worker hands the adapter a `MailDeliveryRequest`, not only rendered text.
It contains the site-scoped delivery id, durable attempt number, delivery
purpose, optional idempotency key and a protected campaign unsubscribe URL.
Campaign adapters emit that URL as `List-Unsubscribe` and
`List-Unsubscribe-Post`; transactional adapters must not invent one. The
worker commits the lease before calling the adapter and records the provider
receipt or retry afterwards. A provider can therefore deduplicate a retry
without receiving a database handle or learning anything about another site.

The self-host composition root includes an HTTPS webhook adapter when
`MAVI_MAIL_WEBHOOK_URL` is set. Its request and response contract is deliberately
small: it receives the typed delivery metadata and returns JSON
`{"reference":"..."}`. SMTP or a vendor SDK can sit behind that endpoint
without entering the Mavi domain crates. If the variable is absent, the
runtime uses a fail-closed adapter and leaves an auditable retry/dead state
rather than silently dropping mail.

The reverse direction is a separate normalized webhook boundary. A trusted
gateway authenticates with `MAVI_MAIL_WEBHOOK_INGEST_TOKEN` and posts one
`mail.provider_events.receive` event at a time. Mavi stores the provider event
ID under site scope, so retries are idempotent. Permanent bounces and
complaints suppress the reader and cancel queued campaign deliveries; transient
bounces do not create permanent suppression. Vendor signature parsing and
provider-specific payload translation remain in the gateway adapter.

## Why building is a port and not an option

The same argument, and the sharpest case of it. A design that has to be built
is a project with its own dependencies and its own command, and running it is
a machine running whatever somebody else wrote — a sandbox, a scheduler and a
quota rather than a function.

None of that belongs in a library anybody installs, and none of it belongs
behind an `if` in the builder either. What ships serves whatever a design put
under `public/`, which is a whole site when a site is plain files. A host that
builds each site's own project hands in its own `Builds`, and nothing above
the port knows which one it got.

## Why there is no reports table

A customer telling whoever runs the machine that a screen is broken is the
**hosting business's** question, not the CMS's. The old table says so itself:
it has a column for whether an assistant said it, so that the same gap
reported about forty sites is one thing to fix rather than forty.

A site somebody installs for themselves has nobody to report to. Where that
belongs is `Outside`, which is the seam a hosting product attaches through.

## The shape of the rule

> This software decides **what** should happen. A host decides **how** it
> happens on its machine.

Everything above follows from that sentence, and so does the answer to the
next one somebody asks.
