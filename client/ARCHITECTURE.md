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
- `src/lib/v1.ts` is the only HTTP plumbing. Shapes come from `@api`, which is
  generated from the server contract and must not be duplicated in a screen.
- `src/components/editor/` and `src/components/mail/` contain shared editor
  primitives, while `src/features/<domain>/` contains complete domain
  screens. The current feature boundaries are `people`, `forms`, `settings`,
  `content`, `media`, `taxonomy`, `shop`, `learning`, `automation`, `boards`,
  `design`, `analytics`, `governance`, and `integrations`.

## Page contract

Every authenticated page should have one page heading, a short description,
and an explicit loading/empty/error state. Actions belong in the page header;
global actions belong in `DashboardHeader`; navigation belongs in the
manifest. A canvas may opt into `WideSurface`, while reading and form pages
use the centered content surface.

URLs remain stable while the information architecture changes. A screen may
move between groups without invalidating bookmarks or API clients.

## Refactor order

1. Keep route guards and generated API calls intact.
2. Move route-level layout and navigation into shell components.
3. Move one domain at a time into `src/features/<domain>`; content, media,
   taxonomy, shop, learning, automation, boards, design, analytics, governance,
   and integrations are the reference shape for new screens.
4. Replace local async states with the shared page contract.
5. Add permission, API, and interaction acceptance tests before deleting the
   old route implementation.
