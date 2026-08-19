/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute, redirect } from "@tanstack/react-router"

import { api } from "@/lib/v1"
import { SetupPage } from "@/features/auth/setup-page"

export const Route = createFileRoute("/setup")({
  loader: async () => {
    const site = await api("open.site").catch(() => null)

    if (site) {
      throw redirect({ to: "/dashboard" })
    }

    return null
  },
  component: SetupPage,
})
