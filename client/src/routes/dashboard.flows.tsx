/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const FlowsPage = lazyRoute(() =>
  import("@/features/automation/flows-page").then(({ FlowsPage }) => ({
    default: FlowsPage,
  })),
)

export const Route = createFileRoute("/dashboard/flows")({
  component: () => <FlowsPage />,
})
