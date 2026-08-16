# audit

Who did what, and when.

**Who reaches it.** The panel, with `audit:view` — a grant of its own, because
somebody who can write posts is not somebody who can read who else has.

**Tables it owns.** `audit_log`, which every other domain writes to and none of
them read.

**Written before anything can answer.** That is the kernel's rule rather than
this module's: a mutation that has not written a row here cannot return a
response, and the router is what enforces it. This is only the reading.

**What changed, as it was written.** Before and after, both, because a row that
says "changed" answers nothing anybody asks it.

**Who, by name where the account is still here.** An account that has gone does
not unwrite what it did, so the id stays and the name is null.

**Three filters and no more**: one person's doing, one kind of thing, one
particular thing. Each is a column with an index behind it, and a field nothing
declares is not a filter.

## What it deliberately does not do

- No writing. Nothing here appends to the log; the domains do, in the
  transaction that made the change.
- No deleting. What it keeps and when it goes is the retention policy's, swept
  by a job like everything else.
- No log of reads. Who looked at what is a different product and a much bigger
  table.
