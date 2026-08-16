# portable

A site's content as a file somebody can keep, and reading one back.

**Who reaches it.** The panel, with `content:view` to take one out and
`content:write` to read one in.

**Tables it owns.** None. It reads and writes the content domain's.

**A bundle says which version it is**, and one from a version this does not
know is refused rather than read hopefully — a reader that guesses is a reader
that half-imports somebody's site. Anything in it this does not recognise is
refused too, because a field silently ignored is a field somebody thought was
carried.

**Ids travel and are not reused.** Reading a bundle makes new rows and
remembers which id in the file became which here, so a post that was under a
category still is.

**Nothing is overwritten.** What is already here under the same address is left
alone and counted, so reading the same bundle twice is one site rather than two
of everything.

**Everything arrives as a draft**, whatever it was. A bundle read into a live
site should not publish forty pages the moment it lands.

**A post is written in one of the site's own languages**, checked here as well
as in the content domain, because the column is text rather than a key and a
bundle is the way a language nobody has would otherwise arrive.

**What it deliberately does not do.**

- No media. The bytes are not in the bundle, and a picture that arrives as a
  broken link is worse than one that was never claimed.
- No settings, no people, no orders. What is portable is what a site wrote.
- No merging. Something already here wins, and the count says how often that
  happened.
