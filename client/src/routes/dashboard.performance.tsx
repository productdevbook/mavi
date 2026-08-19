/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { PerformancePage } from "@/features/analytics/performance-page"

export const Route = createFileRoute("/dashboard/performance")({
  component: PerformancePage,
})
