# media

What a site has uploaded.

**Who reaches it.** The panel, with `media:view`, `media:write`,
`media:delete`. One endpoint is public: the one that serves a file, because
that is how a page's pictures arrive.

**Tables it owns.** `media`, `videos`. The bytes are not in it — they are in whatever is
storing them, and the row says where.

**What may be uploaded is decided by the bytes.** A name ending in `.png`
proves nothing; the first few bytes of a file are what it is. The list is
JPEG, PNG, GIF, WebP, PDF and MP4, and nothing a browser would run is on it —
no SVG, no HTML. A file whose name says one thing and whose bytes say another
is refused, and there is a test that sends exactly that.

**Nothing is kept under the name somebody chose.** A file is stored under its
own id, inside a folder named for the site. What the person called it is kept
in a column, to give back when it is downloaded, and cleaned of anything that
would matter in a header.

**What is served, and how.** The type this machine decided, `nosniff` so that
nothing else decides, and everything that is not a picture handed over as a
download rather than shown. A file cached for a year, since its address
contains its id and a changed file is a new one.

**Limits.** Twenty megabytes a file, and five gigabytes a site unless an
operator has sold it more — a limit on one file and none on the total is a site
filling the disk one legal upload at a time, and a full disk on this machine is
the kubelet evicting Postgres, which takes the whole installation. The total is
counted rather than kept as a running number: one that is written in one place
and decremented in another goes wrong the first time something fails halfway.
Six hundred requests a minute per caller on the public endpoint.

**Storage.** A local folder today. `Store` is an enum with one arm; an object
store is a second, and these three calls — put, get, delete — are what one
answers.

**Retention.** Kept as long as the site keeps it. Deleting is soft, and the
bytes stay until something sweeps them, which does not exist yet and is
written down here rather than left as a surprise.

## Videos

A video is not a picture with a longer name. It is uploaded once, then worked on
for minutes by something that is not this process, and what plays at the end is
not the file that went in. A row in the library cannot say any of that, so it
has a table with a state on it.

- **Handing it over is work**, not a request: a transcoder that is busy, down or
  slow is not a reason for an upload to fail, and the queue is what tries again.
- **A machine with nothing to transcode with says the uploaded file is what
  plays.** For an MP4 a browser can already read that is true, and it beats a
  video that says "working" for ever on a machine that will never work on it.
- **What comes back is signed**, the way a webhook is, and matched by the
  reference that went out — an answer about somebody else's video is an answer
  about nothing. An answer about a video that has since been thrown away is
  still written down, because that arriving repeatedly is somebody's transcoder
  talking to the wrong site.
- **Where it plays is whatever transcoded it says.** Nothing here reads into
  that shape; a renditions list is the transcoder's business.
- **The source is kept** after transcoding, because "make it again" happens and
  the source is what it needs.

**What it deliberately does not do.**

- No thumbnails, no resizing, no transcoding. What that would need is a worker
  and a queue, both of which exist; the work itself does not.
- No dimensions read from the file. The columns are there and nothing fills
  them.
- No deduplication, though the checksum is stored and indexed for the day it
  is wanted.
- No video hosting. `video_asset` in #188 is a different thing from this and
  is not here.
