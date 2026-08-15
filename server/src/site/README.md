# site

The three things a site owes: what it has published, a copy of what it holds
about somebody, and taking that away when they ask.

**Who reaches it.** The panel, with `people:view` to give somebody their copy
and `people:delete` to erase them. `llms.txt` is public and rate limited.

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
