/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute, Outlet, useRouteContext } from "@tanstack/react-router"

import { requireAuth } from "@/lib/auth-guard"
import { Allowed } from "@/components/dashboard/allowed"
import { DashboardShell } from "@/components/dashboard/dashboard-shell"
import { PermissionProvider } from "@/lib/permissions"

export const Route = createFileRoute("/dashboard")({
  beforeLoad: ({ location }) => requireAuth(location.href),
  component: DashboardRoute,
})

function DashboardRoute() {
  const { user } = useRouteContext({ from: "/dashboard" })

  return (
    <PermissionProvider grants={user.grants}>
      <DashboardShell>
        <Allowed>
          <Outlet />
        </Allowed>
      </DashboardShell>
    </PermissionProvider>
  )
}
