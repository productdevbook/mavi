import { createFileRoute, redirect } from "@tanstack/react-router"
import { Loader2 } from "lucide-react"

import { api } from "@/lib/api"

export const Route = createFileRoute("/")({
  loader: async () => {
    // A machine nobody has set up yet has no accounts to sign in with, so the
    // first thing it can offer is setting itself up. Anything else — including
    // not being able to ask — means somebody signs in.
    const setup = await api("setup.status").catch(() => null)
    throw redirect({ to: setup?.initialized ? "/dashboard" : "/setup" })
  },
  pendingComponent: () => (
    <div className="flex min-h-svh items-center justify-center">
      <Loader2 className="size-6 animate-spin text-muted-foreground" />
    </div>
  ),
})
