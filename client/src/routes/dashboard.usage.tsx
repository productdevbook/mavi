/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { UsagePage } from "@/features/analytics/usage-page"

export const Route = createFileRoute("/dashboard/usage")({
  component: UsagePage,
})
