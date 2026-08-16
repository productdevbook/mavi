# What still runs

This is the software that serves the sites today. It was `server/`; the name
changed when the rewrite took that name, and nothing else about it did.

It is here rather than deleted because **deleting it would take features off
the internet.** That is not a feeling about risk, it is a list. What is in this
directory and has no replacement in `server/`:

| Here | Lines | In the rewrite |
|---|---|---|
| `src/edge` | 343 | nothing — this is what serves a visitor the site's own pages |
| `src/mcp` | 726 | nothing — the assistant protocol a site answers |
| `src/publishing` + `src/building` | 1377 | nothing — building a site's pages and putting them out |
| `src/analytics`, `src/reports` | 829 | nothing |
| `src/portable` | 403 | nothing — how a site leaves |
| `src/plugins` | 524 | nothing |
| `src/health` | 325 | nothing |

And one thing that is not a directory: `client/` talks to **this** API. The
rewrite's paths and names are different, so the panel that ships today stops
working the moment this does.

## What has to be true before this goes

1. ~~`server/` produces a binary.~~ It does: `server/mavi` opens the socket,
   applies the migrations, and runs the worker beside itself.
   `server/Dockerfile` makes an image of it.
2. ~~The scheduler exists.~~ It does: `mavi_work::timer` claims a tick with
   the statement that moves it forward, so two workers is one tick. What a
   post given a date needed turned out not to be a scheduler at all — a feed
   asks for `published_at <= now()`, so it goes out on its date without
   anything running.
3. The edge, publishing and the assistant protocol are written in the rewrite,
   or a decision is written down that they are not coming back.
4. `client/` is rewritten against the new API.

Each of those is somebody's afternoon rather than a mystery, and none of them
is done. When the last one is, this directory is one `git rm -r` and the
history keeps it.

## Until then

It builds in CI, under the `old` job, for exactly as long as it is what an
image is made from. `.github/workflows/release.yml` builds `mavi` from here,
and says so in a comment beside the path.
