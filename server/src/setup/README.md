# setup

The first run of a machine: nothing on it yet, and one address that answers
before anybody has signed in.

**Who reaches it.** Nobody, and everybody — there is no account yet to check.
`/api/setup` is rate-limited rather than guarded, because the only thing
standing between a stranger and this door is how fast they can be made to
give up.

**What it owns.** Whether the machine still has nobody to run it, and the one
transaction that gives it somebody: an operator, the one site this
installation is, an owner role holding every grant, and the account able to
sign into that site — made together, so there is never a moment with one and
not the other. Nothing else: an operator's session, invites, and everything
after the first account belong to `console`.

**One site, not the first of many.** The tenant made here is not a seed for
something a console adds to later — there is no way to make a second, on
purpose. What this crate does not have is the capability, not the schema:
`tenant_id`, row-level security and `Host` resolution are unchanged, because
they are what makes this site's isolation real rather than a promise. Running
several is a different product, built on top.

**Taken once.** `where not exists` alone is not enough — two requests arriving
together both read the empty table before either wrote, and both inserted. An
advisory lock is what the second one actually queues behind.

**What anybody is told is the same either way**: a second too late and a
machine that has been running for a year both hear only that the door is
shut.

**No development shortcut.** No flag, no `debug_assertions`, no seeded
account. The only way to the first operator is this door, and the only way to
the second is the first one inviting them — from inside `console`, once it
exists.

**Why this is not under `console`.** Setting a machine up is the one thing
every build of this CMS needs, operator half or not: something has to answer
`/api/setup` before there is anybody to sign in as. `console` — the sites,
the domains, the accounts after the first — is gated behind the `operator`
feature and does not exist in a build without it. This module always does.
