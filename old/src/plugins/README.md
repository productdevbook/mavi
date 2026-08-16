# plugins

What a site plugs into: its own mail server, its own payment provider.

**Who reaches it.** The panel, `settings:view` to see and `settings:write` to
change.

**Tables it owns.** `plugins`.

**Which integrations exist is a list in the code**, not whatever somebody writes
into a table. A key nobody declared is refused, so nothing downstream has to ask
whether a row means anything.

**Two halves.** What a screen may read back, and what is sealed with the
machine's keyring. Which fields are secret is declared beside the plugin, and a
test checks that everything kept is something the plugin asked for. No endpoint
answers with a secret, and what a screen is told is which ones are set.

**What is not sent again is kept.** Changing the address a site sends from does
not mean typing its mail password in a second time.

**A site that plugged in nothing keeps working.** The machine's own settings are
the fallback, so the only thing configuring a plugin changes is whose server
does the work.

**Whether it works is asked, not assumed.** A site whose mail stopped arriving
otherwise finds out from a customer. Nothing is asked of a payment provider,
because the question there is whether a payment goes through and inventing one
to find out is a payment somebody has to refund.

## What it deliberately does not do

- No code. A plugin here is settings for something the CMS already knows how to
  do; running somebody's uploaded code inside a machine that serves other
  people's businesses is a different product.
- No arbitrary settings bag. A table anything can be written into is a table
  nothing can be said about.
