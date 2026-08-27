/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const ContentList = lazyRoute(() =>
  import("@/features/content/content-list").then(({ ContentList }) => ({
    default: ContentList,
  })),
)

export const Route = createFileRoute("/dashboard/content/$kind")({
  component: ContentRoute,
})

/** Whatever this site said it publishes: courses, packages, properties. */
function ContentRoute() {
  const { kind } = Route.useParams()
  return <ContentList kind={kind} />
}
