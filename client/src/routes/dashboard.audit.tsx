/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const AuditPage = lazyRoute(() =>
  import("@/features/governance/audit-page").then(({ AuditPage }) => ({
    default: AuditPage,
  })),
)

export const Route = createFileRoute("/dashboard/audit")({
  component: () => <AuditPage />,
})
