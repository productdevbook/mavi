/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const PerformancePage = lazyRoute(() =>
  import("@/features/analytics/performance-page").then(({ PerformancePage }) => ({
    default: PerformancePage,
  })),
)

export const Route = createFileRoute("/dashboard/performance")({
  component: () => <PerformancePage />,
})
