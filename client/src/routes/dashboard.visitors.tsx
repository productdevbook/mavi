/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const VisitorsPage = lazyRoute(() =>
  import("@/features/analytics/visitors-page").then(({ VisitorsPage }) => ({
    default: VisitorsPage,
  })),
)

export const Route = createFileRoute("/dashboard/visitors")({
  component: () => <VisitorsPage />,
})
