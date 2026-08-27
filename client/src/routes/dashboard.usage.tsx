/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const UsagePage = lazyRoute(() =>
  import("@/features/analytics/usage-page").then(({ UsagePage }) => ({
    default: UsagePage,
  })),
)

export const Route = createFileRoute("/dashboard/usage")({
  component: () => <UsagePage />,
})
