/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute, redirect } from "@tanstack/react-router"

import { nextApi } from "@/lib/server-next"
import { SetupPage } from "@/features/auth/setup-page"

export const Route = createFileRoute("/setup")({
  loader: async () => {
    const setup = await nextApi("setup.status").catch(() => null)

    if (setup?.initialized) {
      throw redirect({ to: "/dashboard" })
    }

    return null
  },
  component: SetupPage,
})
