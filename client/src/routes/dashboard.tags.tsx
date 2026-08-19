/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { TagsPage } from "@/features/taxonomy/tags-page"

export const Route = createFileRoute("/dashboard/tags")({
  component: TagsPage,
})
