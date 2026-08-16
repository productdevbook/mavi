/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { requireAuth } from "@/lib/auth-guard"
import { MaviEditor } from "@/components/editor/mavi-editor"

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
  return (
    <MaviEditor
      postId={null}
      locale={locale}
      translationOf={translationOf}
      kind={kind}
    />
  )
}
