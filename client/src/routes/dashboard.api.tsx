/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { ApiPage } from "@/features/integrations/api-page"

export const Route = createFileRoute("/dashboard/api")({
  component: ApiPage,
})
