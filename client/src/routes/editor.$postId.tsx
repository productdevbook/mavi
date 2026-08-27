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

export const Route = createFileRoute("/editor/$postId")({
  beforeLoad: ({ location }) => requireAuth(location.href),
  component: RouteComponent,
})

function RouteComponent() {
  const { postId } = Route.useParams()
  const { user } = Route.useRouteContext()
  return (
    <PluginProvider>
      <PermissionProvider grants={user.grants} permissions={user.permissions}>
        <Allowed>
          <MaviEditor postId={postId} />
        </Allowed>
      </PermissionProvider>
    </PluginProvider>
  )
}
