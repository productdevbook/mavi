# publishing

The project a site's pages are built from, and putting it live.

**Who reaches it.** The panel, with `design:*` to write and `publish:*` to put
it live. Nothing here is public.

**Tables it owns.** `theme_files`, `publishes`.

**Only `src/` and `public/` can be written.** What decides how a site is built
is not a thing a site edits: a build config a customer can change is a build
this machine runs. The database says so with a check constraint and the handler
says so as well.

**Nothing writes to what is being served.** Saving goes to a branch; `live` is
what a publish produces. There is no path from an editor to the pages a visitor
sees that does not go through a build.

**A publish is the whole of a site.** What is on the branch becomes what is
live, and anything live the branch does not have goes — a publish is not a
patch, so a file deleted on the branch is a file gone from the site.

**One at a time**, said by a partial unique index rather than by a
lock in a process: two publishes racing is two builds writing the same output,
and an index is a thing that holds across replicas.

**What a build cost is written where the bill is worked out from.** Seconds on
the publish, and the day's total into `usage_events` — a build nobody counted is
a build nobody bills.

**The build is handed to a runner.** An arm on the network where one is
configured, and where none is, the files are the site — which is what a site
with no generator has. The claim is committed before the build starts, so the
panel can see it building and a cancel has something to cancel.

**A build that fails leaves what is live alone**, and says why. Half a site is
worse than an old one.

**A publish can be told not to.** One that has not started never runs; one that
is building has what comes back thrown away rather than put live, and a publish
that has already finished cannot be cancelled.

**Six publishes an hour.** A build is minutes of every core this machine has and
the queue is one queue, so a site publishing on every save would put everybody
else's site behind it. Publishing twice at once is refused separately, by the
database.

**What it deliberately does not do.**

- The generator itself is a container this repository does not hold. What is
  here is everything around it.
- No previews at an address. A branch exists and nothing serves it yet.
- No history beyond the last version of a file per branch, and no way back to
  an earlier one.
