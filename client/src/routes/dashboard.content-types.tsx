/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { ContentTypesPage } from "@/features/content/content-types-page"

export const Route = createFileRoute("/dashboard/content-types")({
  component: ContentTypesPage,
})
