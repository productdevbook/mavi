/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { VisitorsPage } from "@/features/analytics/visitors-page"

export const Route = createFileRoute("/dashboard/visitors")({
  component: VisitorsPage,
})
