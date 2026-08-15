import { createFileRoute, Outlet } from "@tanstack/react-router"

import { requireAuth } from "@/lib/auth-guard"
import { Allowed } from "@/components/dashboard/allowed"
import { DashboardShell } from "@/components/dashboard/dashboard-shell"
import { PermissionProvider } from "@/lib/permissions"

export const Route = createFileRoute("/dashboard")({
  beforeLoad: ({ location }) => requireAuth(location.href),
  component: () => (
    <PermissionProvider>
      <DashboardShell>
        <Allowed>
          <Outlet />
        </Allowed>
      </DashboardShell>
    </PermissionProvider>
  ),
})
