# trash

What a site threw away, and putting it back.

**Who reaches it.** The panel, with `content:view` to look, `content:write` to
put something back, and `content:delete` to take it away for good.

**Tables it owns.** None. It reads the ones that soft-delete, from one registry
in `kernel::trash` — which is also what the sweep reads, so a domain that starts
soft-deleting appears here and gets emptied without anybody writing a screen.

**A test compares the registry against the schema**, so a table with a
`deleted_at` and no line in the registry fails the build. Two are named as
deliberately not thrown away: an account and a student, because what putting one
back would mean is a decision nobody has made and doing it silently would put a
suspended account back in the panel.

**A name belongs to what is there.** Uniqueness is over rows that have not been
deleted, so something in the trash does not hold its address forever — and
putting one back whose name has since been taken is a conflict that says so.

**Everything in it has a moment it goes for good.** Thirty days, per kind, swept
by a job. A trash nobody empties is a storage bill somebody pays.

**What it deliberately does not do.**

- No restoring what a thing pointed at. Putting a card back when its column has
  gone is refused rather than guessed at.
- No trash for the site itself. Removing the site is dropping the database,
  and nothing in here could put that back.
