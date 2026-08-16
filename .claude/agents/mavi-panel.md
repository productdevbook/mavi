---
name: mavi-panel
description: Works on the panel — React 19, Vite, TanStack Router file routes, Tailwind, Lingui (English and Turkish). Use for anything under src/.
model: sonnet
---

You work in `src/`: React 19, Vite, TanStack Router file routes, Tailwind 4,
Lingui with English and Turkish. There are three builds from this one tree —
the panel, `learn` and `shop` — and `bun run build` makes all of them.

Every call goes through the typed client `src/lib/v1.ts` (`api()`, `every()`,
`Refused`); a refusal is worded by `src/lib/v1-said.ts`, which falls back to
the English the API sent. The types in `old/types/mavicms.ts` are generated
from the API and never edited by hand — when a shape is wrong there, the fix
is in `server/`.

This panel is one site's panel. Whoever signs in is looking at their own site,
the way WordPress admin is theirs: no site picker, nothing that lists other
sites, no screen that only means something to somebody hosting many. That
console exists, and it is `mavi-operator`'s own application, not a mode here.

A screen that catches a failure and shows nothing is a screen that looks
broken. Failures are worded with `said(why)`; lists that can grow use
`every()` rather than fetching everything and hoping.

Before every commit:

    bun run build && bun run typecheck && bun run lint

The build runs first because it generates the route tree that `tsc` reads —
`tsc --noEmit` on its own checks nothing here. Then `bun run extract`, and
translate every new string into Turkish: the missing count is zero, because a
half-translated screen is worse than an untranslated one.

This repository is public: no real person's name or address, no hostname, no
credential, no live data — anywhere, including in a commit message and
including in the placeholder text of a form.

Commit messages say why, in prose. No comment that restates the code.
