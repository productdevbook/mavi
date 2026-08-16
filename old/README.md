# What still runs

This is the software that serves the sites today. It was `server/`; the name
changed when the rewrite took that name, and nothing else about it did.

It is here rather than deleted because **deleting it would take features off
the internet.** That is not a feeling about risk, it is a list. What is in this
directory, and what of it the rewrite can do:

| Here | Lines | In the rewrite |
|---|---|---|
| `src/edge` | 343 | ~~nothing~~ `server/mavi-edge` and `mavi-everything/src/showing.rs` |
| `src/mcp` | 726 | ~~nothing~~ `server/mavi-assistant` and `mavi-everything/src/assistant.rs`, and none of its seven hundred lines came with it: a tool is an endpoint |
| `src/publishing` + `src/building` | 1377 | ~~nothing~~ publishing is one row, and how a site is built is the `Builds` port. What ships serves what a design put under `public/`; a host that runs each site's own generator hands in its own. |
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
   - ~~The edge.~~ Written: `mavi-edge` decides which file answers an address
     and where a renamed page went, with no database anywhere near it, and
     `mavi-everything/src/showing.rs` is where the bytes come from. Under
     `/api` the API still answers as the API; everything else is the site.
   - ~~Putting a build out.~~ Written, and smaller than it was: what is
     published is one row, so there is no job that "puts it live" afterwards
     and no moment between the two where a site serves neither.
   - ~~Building.~~ Decided rather than written, which is what this item
     allows for: **how** a site is built is a port. What ships serves what a
     design put under `public/`, which is a whole site when a site is plain
     files. Running a site's own generator is a machine running somebody
     else's code — a sandbox, a scheduler and a quota — and that is the
     host's, not a library's.
   - ~~The assistant protocol.~~ Written, and without a list of tools in it.
     An assistant is a caller, a tool is an endpoint, and both ways in go
     through the same `Door::call` — so "forbidden in the panel, allowed over
     there" is impossible rather than unlikely.
4. `client/` is rewritten against the new API.
   - ~~It has something to be written against.~~ `client/src/api/mavi.ts` is
     generated from the description — a hundred and eighteen types and a
     hundred and two calls, each carrying the sentence its endpoint or its
     field already had. A test writes it and compares, so a shape that moves
     and a panel that has not caught up is a red build rather than a screen
     that breaks in somebody's browser.
   - **The screens.** Forty-three routes, of which forty-one talk to the API
     this replaces. `src/lib/v1.ts` is already the right shape — it keys its
     calls by name and takes its types from `@api` — so what is left is
     pointing that alias at the new file and following the type errors.

Three of the four are done, and the fourth has what it needs to start. When
the last one is, this directory is one `git rm -r` and the history keeps it.

## Until then

It builds in CI, under the `old` job, for exactly as long as it is what an
image is made from. `.github/workflows/release.yml` builds `mavi` from here,
and says so in a comment beside the path.
