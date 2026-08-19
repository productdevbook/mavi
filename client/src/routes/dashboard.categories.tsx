/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { CategoriesPage } from "@/features/taxonomy/categories-page"

export const Route = createFileRoute("/dashboard/categories")({
  component: CategoriesPage,
})
