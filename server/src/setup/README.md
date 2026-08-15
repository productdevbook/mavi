# setup

The first run of a machine: nothing on it yet, and one address that answers
before anybody has signed in.

**Who reaches it.** Nobody, and everybody — there is no account yet to check.
`/api/setup` is rate-limited rather than guarded, because the only thing
standing between a stranger and this door is how fast they can be made to
give up.

**What it owns.** Whether the machine still has nobody to run it, and the one
transaction that gives it somebody: the site's own settings, an owner role
holding every grant, and the account able to sign in — made together, so there
is never a moment with one and not the other. Nothing else: invites and every
account after the first belong to `people`.

**One site, not the first of many.** There is no row saying the site exists:
the installation is the site, so what this writes is the things a site is made
of. There is no way to make a second, and nothing left that could tell two
apart if there were. Running several is a different product, and it is a second
installation rather than a second row in this one.

**Taken once.** `where not exists` alone is not enough — two requests arriving
together both read the empty table before either wrote, and both inserted. An
advisory lock is what the second one actually queues behind. The row it is
asked about is `site_settings`: it is written once, it is the first thing
written, and there is at most one of it.

**What anybody is told is the same either way**: a second too late and a
machine that has been running for a year both hear only that the door is
shut.

**It writes an audit row like anything else.** It used to answer with a receipt
made out of nothing and write a line to a log with no reader. Now the owner it
just created is the actor, and the receipt is the one the router checks for —
so the gate that refuses a change which answered and recorded nothing covers
this door too, which it never did before.

**No development shortcut.** No flag, no `debug_assertions`, no seeded
account. The only way to the first account is this door, and the only way to
the second is the first one inviting them.

**Locked out.** There is no second account to be recovered through — the
operator that used to be written here could never sign in, so it was not one
either. The way back in is `mavi reset-password <address>` on the host, which
reads the new password from standard input, clears the second factor and ends
every session that account had.
