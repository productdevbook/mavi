# people

Who is on a site, and what they may do.

**Who reaches it.** The panel, with `people:*`. Three endpoints are public: asking
to reset a password, choosing one, and proving an address — all rate limited,
the first saying the same thing whether or not there is an account behind the
address.

**Tables it owns.** `users`, `roles`, `tickets`.

**A role is data.** It carries a set of grants the site edits, and the policy
asks only whether the grant needed is in it. Adding a role is not a deploy.

**Nobody hands out more than they hold.** A role can only be made with grants
the person making it holds themselves — otherwise `people:write` is a way
around every other check there is.

**Nobody changes what they themselves are**, and nobody removes their own
account. Both would leave a site with nobody able to put it back.

**The last owner is never stranded**, by any route. Deleting the account
(`remove`), erasing it (`site::erase`), suspending it, or moving it to another
role (`change`) all ask `refuse_if_last_owner` first — one answer to "is
anybody else left who could sign in as an owner" rather than four call sites
guessing at it separately. "Anybody else" means able to sign in: active, not
deleted, with a password chosen, the same test `auth::sign_in` itself uses —
an owner still sitting on an unused invitation, or already suspended, does
not count as one still standing.

**A ticket is good once, for what it was minted for.** An invitation, a reset
and an address proof are the same row — hashed, dated, spent when it is used,
and any earlier one for the same purpose spent when a new one is made — but
`purpose` is in the query that redeems one, not checked afterward: a ticket of
the wrong purpose is not found rather than found and refused. An invitation or
a reset chooses a password, which also proves the address it was sent to. An
address proof does only that — it does not choose a password, does not
activate an account still sitting on an invitation, and does not revoke
sessions, because proving an address is not a credential change.

**The letter matches the ticket.** `invite` mints an `invitation` ticket and
presses the `invitation` letter; `ask_to_reset` mints a `password_reset`
ticket and presses `password`; changing an address mints an `email_proof`
ticket and presses `email_proof`. A ticket pressed under the wrong kind is a
person told they were invited when their address was only changed.

**Suspending takes away what they are holding.** The state and the sessions go
together; changing one without the other leaves somebody signed in for a month.

**Changing a password ends every session** that was open, which is the point of
changing it.

**Retention.** An account is kept as long as the site keeps it. Tickets go
seven days after they expire or are spent, swept by `sessions.sweep`.

**What it deliberately does not do.**

- No two-factor authentication (#146).
- No sign-in with somebody else's account, no OAuth.
