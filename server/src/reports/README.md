# reports

Somebody saying a screen is broken, and the answer coming back.

**Who says something.** Anybody signed in on a site, with no grant asked for.
The machine's own screens see every site's and write the answers.

**Tables it owns.** `reports`.

**Saying is ungated, deliberately.** What a grant would gate here is somebody
telling the people who run the machine that something is broken, and a person
who cannot do that is a person who telephones instead. Ten an hour, so a loop
in a script does not become the machine's inbox.

**Reading the list is not.** `GET /api/reports` asks for `audit:view` — the
same grant as the audit log, because a report is the same shape of thing: a
record of what happened, including what `environment` gathered about the
person who said it, and not everybody with an account should read that. An
editor limited to their own posts does not get to read every gap anybody on
the site has reported just for holding an account; an assistant connected
through MCP with a narrower grant is filtered the same way. Writing stays
open so the gap can be reported in the first place — the two are not the same
decision, and giving the read the write's openness was the bug.

**An assistant may say something too.** A key is refused the things that outlive
the day it lasts, and this is not one of them — but which said it is written
down, because an assistant reporting the same gap in forty sites is one thing to
fix rather than forty.

**The answer goes beside what was said.** Not into a mailbox: an address changes
and an account goes, and the thing they said stays where they said it.

**"We are looking at it" is an answer.** Answering without closing leaves it
open, because a report that is closed by being replied to is one nobody comes
back to.

**Kept a year, like the audit log.** `said_by` names who said it and
`environment` gathers what their browser was doing, so a report is somebody's
own data and not kept forever — `reports.sweep` takes rows older than 365
days, on the same schedule as `audit.sweep`.

## What it deliberately does not do

- No screenshots. A picture of a broken screen may hold somebody's inbox, their
  customer list, or half a database, and this machine already has the site.
- No mail. The answer is where the question was, and a site that wants to know
  the moment it arrives is a webhook away rather than an inbox away.
