# analytics

What a site was asked for, without keeping who asked.

**Who reaches it.** The panel, with `settings:view`, to read the numbers. One
public endpoint: the beacon a page sends, rate limited.

**Tables it owns.** `page_views`, `visitor_marks`, `vitals`.

**Counted rather than logged.** A row per visit is a table nobody can afford,
and nothing here needs to know which visit was which: a day, a path, a count.

**No address is kept.** Today's salt — the site and the day — hashes the
address into a mark. The mark answers "has this day seen them before", and goes
two days later. There is nowhere in these three tables for an address, an agent
or a name, and there is a test that reads the columns and says so.

**A measurement that nothing could have measured is left out.** Negative, or
longer than ten minutes, is a browser being odd rather than a page being slow.

**Retention.** Marks two days. Measurements ninety. Counts stay, because a
count of nobody in particular is not somebody's data.

**What it deliberately does not do.**

- No referrers, no countries, no devices. Each is a thing that identifies
  somebody in combination with the others, and none of them is needed to know
  whether a page is read.
- No live view, no sessions, no funnels.
- The measurements are stored and nothing reads them yet; the panel screen that
  would is not written.
