# building

Running the thing that turns a site's files into its pages.

**Who reaches it.** Nobody, directly. Publishing calls it when a generator is
configured for this machine.

**Tables it owns.** None.

**What the old machine got wrong, and what this does instead.**

- **Every build ran as one user, in folders every other build could read and
  write.** One customer's `postinstall` could read every other customer's source
  and change their live site. Here a build gets a directory of its own, named
  for the site and the publish, made `0700` by this process; one that already
  exists is refused rather than reused, and it is removed however the build went.
- **Nothing in the code limited how many ran at once** — only a manifest did,
  and three at once took the machine off the air. One at a time is a semaphore
  held for the whole build, and raising it is an environment variable rather
  than a hope.
- **A timed-out child was dropped, not killed**, and kept its cores for hours.
  Twenty minutes, `kill_on_drop`, and a log that says it was stopped.
- **The environment went straight through.** This process holds the database
  password, the keyring and every provider's key, and what runs here is a command
  a customer can change; it gets `PATH`, `HOME` and `CI`, and a test reads what
  the build saw to prove it.

**A program and its arguments, never a shell line.** A shell line is how a
site's own name ends up being executed.

**A build that produced nothing failed**, whatever it said on the way out.
Putting an empty folder live is a site that has gone.

## What it deliberately does not do

- No user namespaces, no cgroups, no seccomp. Those belong to whatever runs this
  process and cannot be written here; what is here is every guard that can be.
- No caching of what an install downloaded. It is the obvious next thing and it
  is a shared directory, which is the whole subject of this file.
