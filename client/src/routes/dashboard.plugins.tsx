/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute, redirect } from "@tanstack/react-router"

/**
 * Kept as a compatibility address for old bookmarks. The previous screen was
 * a local-only placeholder with no server contract, so it must not advertise
 * saves that never reach the installation.
 */
export const Route = createFileRoute("/dashboard/plugins")({
  beforeLoad: () => {
    throw redirect({ to: "/dashboard/api" })
  },
})
