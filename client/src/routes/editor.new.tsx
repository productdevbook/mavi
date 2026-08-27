/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { requireAuth } from "@/lib/auth-guard"
import { lazyRoute } from "@/lib/lazy-route"
import { Allowed } from "@/components/dashboard/allowed"
import { PermissionProvider } from "@/lib/permissions"
import { PluginProvider } from "@/lib/plugins"

const MaviEditor = lazyRoute(() =>
  import("@/components/editor/mavi-editor").then(({ MaviEditor }) => ({
    default: MaviEditor,
  })),
)

export const Route = createFileRoute("/editor/new")({
  // The language has to be settled before the first autosave creates the row —
  // picking it afterwards would leave the post in the wrong language.
  validateSearch: (
    search: Record<string, unknown>
  ): { locale?: string; translationOf?: string; kind?: string } => ({
    locale: typeof search.locale === "string" ? search.locale : undefined,
    translationOf:
      typeof search.translationOf === "string" ? search.translationOf : undefined,
    // Any kind this site publishes, not one of the two there used to be.
    // This line was written when there were only posts and pages, and it
    // silently dropped every other kind — so Add on the courses page arrived
    // at an editor writing a post, with none of a course's fields on it and
    // nothing to say why.
    kind: typeof search.kind === "string" && search.kind ? search.kind : undefined,
  }),
  beforeLoad: ({ location }) => requireAuth(location.href),
  component: NewPostRoute,
})

function NewPostRoute() {
  const { locale, translationOf, kind } = Route.useSearch()
  const { user } = Route.useRouteContext()
  return (
    <PluginProvider>
      <PermissionProvider grants={user.grants} permissions={user.permissions}>
        <Allowed>
          <MaviEditor
            postId={null}
            locale={locale}
            translationOf={translationOf}
            kind={kind}
          />
        </Allowed>
      </PermissionProvider>
    </PluginProvider>
  )
}
