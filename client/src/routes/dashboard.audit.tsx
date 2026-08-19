/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { AuditPage } from "@/features/governance/audit-page"

export const Route = createFileRoute("/dashboard/audit")({
  component: AuditPage,
})
