# edge

Serving the pages a build published.

**Who reaches it.** Anybody. It is what answers on a site's own address for
anything that is not one of its endpoints.

**Tables it owns.** None. What is live is the newest publish that says it is,
and what it produced is in the store under that publish's own id.

**One deployment.** A site's pages are served by the same process that serves
its panel, so a site made this morning appears the moment it is published rather
than when somebody adds a container for it.

**Going live is an id changing.** A build writes into `builds/{publish}/…` and
nothing overwrites what is being served: a publish that fails halfway leaves the
old build exactly where it was, and rolling back would be pointing at an older
id rather than rebuilding.

**A folder is its index**, and anything with a dot in it is a file by name.

**Only what a theme keeps in `public/`** is a page. `src/` is what a generator
reads, and serving it would put a site's source on its own address — there is a
test that asks for exactly that and expects nothing back.

**The site's own 404**, where it published one.

**A minute of caching.** An address does not carry the id of the build that
answered it, so a longer one would serve yesterday's page out of somebody's
browser after a publish. Fingerprinted names in a theme are the fix for that,
not a bigger number here.

**Nothing here decides what a page may do.** The policy this machine puts on its
own answers is taken off: a site's page loading its own stylesheet is not this
machine's business, and the page is served from the site's own address.

## What it deliberately does not do

- No compression yet, and no range requests. Both belong here and neither is
  written; what is in front of this in production already does the first.
- No cache in memory. Every page is a read from the store, which for a disk is
  a read from the page cache anyway — this becomes worth doing the day the store
  is somewhere across a network.
