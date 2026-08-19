/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { FormsPage } from "@/features/forms/forms-page"

export const Route = createFileRoute("/dashboard/forms")({
  component: FormsPage,
})
