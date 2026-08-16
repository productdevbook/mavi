/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { Outlet, createRootRoute } from "@tanstack/react-router"
import { TanStackRouterDevtools } from "@tanstack/react-router-devtools"

import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"

export const Route = createRootRoute({
  component: RootLayout,
})

function RootLayout() {
  return (
    <TooltipProvider delay={400}>
      <Outlet />
      {/* Mounted once, here, because a toast with no Toaster on the page is
          silence: every screen that reported a failure was reporting it to
          nobody. The editor had one of its own and was the only screen where
          anything was ever said. */}
      <Toaster position="bottom-center" />
      {import.meta.env.DEV && (
        <TanStackRouterDevtools position="bottom-right" />
      )}
    </TooltipProvider>
  )
}
