/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { ContentList } from "@/features/content/content-list"

export const Route = createFileRoute("/dashboard/content/$kind")({
  component: ContentRoute,
})

/** Whatever this site said it publishes: courses, packages, properties. */
function ContentRoute() {
  const { kind } = Route.useParams()
  return <ContentList kind={kind} />
}
