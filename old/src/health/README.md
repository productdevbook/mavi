# health

Whether the site is well, and whether the address it was started with works.

**Who reaches it.** The panel, with `settings:view`.

**Tables it owns.** `domain_checks`.

**Everything here is a question somebody asks after something has gone wrong**,
so the answers are kept where a screen can show them rather than worked out by
whoever is on the phone.

**A handful of checks, not everything measurable.** Whether anything is
published, whether the last publish failed, how many pages have warnings on
them, whether a site's own mail server is working, and whether its addresses
answer. A health screen with forty rows is one nobody reads.

**An address is checked on a schedule**, because it is somebody else's DNS being
asked and a screen must not wait for that. What is written down is whether the
name resolves and whether this machine answers on it — and *nothing looked yet*
is null rather than false, because that is not the same as broken.

**The certificate is not this machine's to say.** The ingress asks for them and
holds them; a second thing guessing at expiry is a second thing to be wrong.

**The operator's report is counted rather than listed.** "Is anything stuck" is
a number — dead jobs, waiting jobs, addresses that do not answer — and which
ones is the list beside it.

## What it deliberately does not do

- No uptime history. What was wrong last Tuesday is a monitoring product's
  question, and this machine already has one pointed at it.
- No alerting. The numbers are published for something that alerts to read.
