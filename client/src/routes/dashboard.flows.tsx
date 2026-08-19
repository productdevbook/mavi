/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { FlowsPage } from "@/features/automation/flows-page"

export const Route = createFileRoute("/dashboard/flows")({
  component: FlowsPage,
})
