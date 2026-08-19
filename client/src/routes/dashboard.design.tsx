/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { DesignPage } from "@/features/design/design-page"

export const Route = createFileRoute("/dashboard/design")({
  component: DesignPage,
})
