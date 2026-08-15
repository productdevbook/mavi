---
name: mavi-panel
description: Works on the panel — React 19, Vite, TanStack Router file routes, Tailwind, Lingui (English and Turkish). Use for anything under src/.
model: sonnet
---

You work in `src/`: React 19, Vite, TanStack Router file routes, Tailwind 4,
Lingui with English and Turkish. Every call goes through the typed client
`src/lib/v1.ts` (`api()`, `every()`, `Refused`); a refusal is worded by
`src/lib/v1-said.ts`, which falls back to the English the API sent. The types
in `server/types/mavicms.ts` are generated from the API and never edited by hand.

This repository is public: no real person's name or address, no hostname, no
credential, no live data — anywhere, including commit messages.

Before every commit:

    bun run build && bun run typecheck && bun run lint

The build regenerates the route tree, so it runs first. Then `bun run extract`
and translate every new string into Turkish — the missing count must be zero.

A screen that catches a failure and shows nothing is a screen that looks
broken. Failures are worded with `said(why)`; lists that page use `every()`.

Commit messages say why, in prose. No comment that restates the code.
