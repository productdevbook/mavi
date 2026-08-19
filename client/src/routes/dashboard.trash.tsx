/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { TrashPage } from "@/features/governance/trash-page"

export const Route = createFileRoute("/dashboard/trash")({
  component: TrashPage,
})
