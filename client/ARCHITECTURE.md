# Mavi panel client architecture

The panel is a Vite/TanStack Router application mounted at `/admin/`. It is a
site client, not a second domain layer: the server remains the authority for
scope, authorization, validation, and audit.

## Boundaries

- `src/routes/` owns URL parameters, route guards, and composition only.
- `src/components/dashboard/` owns the authenticated site shell and reusable
  page language (headers, loading, empty, and error states).
- `src/components/ui/` contains shadcn/base primitives. Domain components do
  not add business rules to these primitives.
- `src/lib/dashboard-navigation.ts` is the canonical panel information
  architecture. Each destination declares its capability next to its URL;
  the renderer only draws destinations the current grants allow.
- `src/lib/v1.ts` is the compatibility HTTP boundary for the old `/api/*`
  contract during the migration. No new screen may add a call to it.
- `src/lib/api.ts` is the canonical HTTP boundary for the clean `/api/v1/*`
  contract. Screens migrating to the rewrite use generated operation IDs from
  `@api`; bearer session storage, refusal handling and
  cursor walking stay in this boundary.
- `src/lib/media.ts` is the media delivery boundary. Published
  content stores `/public/v1/files/{id}` URLs only; private panel previews use
  an authenticated download and revoke their object URL when unmounted.
- `src/lib/upload.ts` is a thin typed media adapter. Its caller must choose
  visibility, so an image inserted into published content is public while
  course video remains private by default.
- `src/lib/auth.ts` is the authenticated application boundary on top of
  `api.ts`. Login stores only the bearer token, then obtains
  the current person and effective Cedar grants from `auth.session.current`.
  The client never rebuilds access by aggregating roles, and it never treats a
  cached session as authorization.
- `src/api/server.ts` is generated from
  `server/mavi-http/contracts/mavi.ts`. CI compares the files in both
  directions so the panel cannot silently drift from the Rust contract.
- `src/api/legacy.ts` and `@legacy-api` are temporary compatibility artifacts
  for screens whose domain slice has not yet moved to the canonical contract.
- Generated shapes must not be duplicated in a screen. Cursor helpers are only
  available for operations whose generated answer is a page.
- `src/components/editor/` and `src/components/mail/` contain shared editor
  primitives, while `src/features/<domain>/` contains complete domain
  screens. The current feature boundaries are `auth`, `dashboard`, `people`,
  `forms`, `settings`, `content`, `media`, `taxonomy`, `shop`, `learning`,
  `automation`, `boards`, `design`, `analytics`, `governance`, `mail`,
  `integrations`, and `portability`.

## Page contract

Every authenticated page should have one page heading, a short description,
and an explicit loading/empty/error state. Actions belong in the page header;
global actions belong in `DashboardHeader`; navigation belongs in the
manifest. A canvas may opt into `WideSurface`, while reading and form pages
use the centered content surface.

URLs remain stable while the information architecture changes. A screen may
move between groups without invalidating bookmarks or API clients.

## Refactor order

1. Keep route guards and the generated API boundary intact. Setup, login,
   password reset, root redirect, authenticated shell, content, taxonomy and
   media use `api`; the `@legacy-api` boundary is compatibility-only for
   domains that have not yet received their canonical server implementation.
2. Move route-level layout and navigation into shell components.
3. Move one domain at a time into `src/features/<domain>`; auth, dashboard,
   content, media, taxonomy, shop, learning, automation, boards, design,
   analytics, governance, mail, integrations, and portability are the reference
   shape for new screens.
4. Replace local async states with the shared page contract.
5. Add permission, API, and interaction acceptance tests before deleting the
   old route implementation.
6. Migrate each screen from `@legacy-api` to `@api`, then delete the
   compatibility boundary only after the whole panel runs against the
   canonical server contract.
