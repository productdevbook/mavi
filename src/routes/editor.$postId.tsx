/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { requireAuth } from "@/lib/auth-guard"
import { MaviEditor } from "@/components/editor/mavi-editor"

export const Route = createFileRoute("/editor/$postId")({
  beforeLoad: ({ location }) => requireAuth(location.href),
  component: RouteComponent,
})

function RouteComponent() {
  const { postId } = Route.useParams()
  return <MaviEditor postId={postId} />
}
