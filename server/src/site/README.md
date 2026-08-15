# site

The three things a site owes: what it has published, a copy of what it holds
about somebody, and taking that away when they ask. And what it is itself:
its name, how much room it has left, and — at `/api/site/usage` — what it is
holding and what it has done.

**Who reaches it.** The panel, with `people:view` to give somebody their copy
and `people:delete` to erase them. `settings:view` reads `/api/site` and
`/api/site/usage`; `settings:write` renames the site. `llms.txt` is public and
rate limited.

**`/api/site/usage` is a number about this installation, never a number about
what somebody owes.** Storage used and by what, mail attempted and delivered
and bounced, recent builds and how long each took, the queue's backlog and how
old the oldest waiting job is, and rows by kind. No prices, no plans, no
quota — `storage_limit_bytes` is what a site may take and stays out of this;
this is only what it has taken. It replaces what `usage_events` and `charges`
(#11, #35) used to count for a bill: `mail_sent` and `build_seconds` are read
here from `email_log` and `publishes` themselves rather than from a daily
tally kept beside them, and `storage_bytes_day`, `bandwidth_bytes`, and
everything in `charges` and `ledger` are the second kind and answer nothing
here.

**Rows by kind counts every table, for now.** `rows.exact` is always `true`
today. It should not always be: a `count(*)` over every table on every load
of this screen is a page that gets slower exactly as the site gets bigger —
the moment nobody wants to be waiting on it — and a large table should read
`pg_class.reltuples`, which Postgres already keeps for its own planner,
instead of being walked. An attempt at exactly that could not be made
trustworthy in the time this endpoint had for it — #72 has what was tried and
what is still unknown — so it was cut rather than shipped half-verified, the
same way `/api/overview`'s own `count(*)` over eight tables is open as #60
rather than fixed by guessing. `rows.exact` stays on the shape so a caller
does not have to change again once one of those lands. This is also a
whole-table number rather than a tenant-scoped one, since nothing in
Postgres's own catalog is filtered by row-level security — the same number
as a real count for as long as `/api/setup` stays the only place a tenant is
ever made.

**Tables it owns.** None. It reads across the domains that hold somebody's
data, from one list.

**One list of where a person is.** Both the copy and the erasure are read from
it, and a test compares that list against the retention policies — a domain
that adds a table holding an address and does not add a line here fails, which
is how "we forgot that table" is prevented rather than apologised for.

**A copy carries no secrets.** Password hashes and tokens are taken out on the
way past, and there is a test that checks the answer for one.

**Erasing keeps the bill and empties it of the person.** An order that vanishes
is a bill nobody can explain; the rule that says keep the record and the rule
that says remove the person are both true, and this is where they meet.

**Erasing the site's only owner is refused, not blanked.** Whether an address
belongs to the last account holding the owner role is `people`'s question to
answer, not a second copy of it here — this domain calls into
`people::refuse_if_last_owner` before it touches the `users` row, the one
place `people::remove` asks the same question. Nothing is erased at all when
it refuses: leaving some tables emptied and `users` untouched would be the
site disagreeing with itself about what happened to one address.

**`llms.txt` is written from what is published**, not from a file somebody has
to remember to update, and a draft is not in it.

**What it deliberately does not do.**

- No import. Export is half of `portable` in #188 and the other half is its own
  change: reading somebody else's export back in needs a version on it and a
  decision about what happens to ids.
- No self-service. Somebody with `people:*` asks on a person's behalf; a
  visitor asking for their own copy needs an identity, and a site's visitors do
  not have one.
