/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const CategoriesPage = lazyRoute(() =>
  import("@/features/taxonomy/categories-page").then(({ CategoriesPage }) => ({
    default: CategoriesPage,
  })),
)

export const Route = createFileRoute("/dashboard/categories")({
  component: () => <CategoriesPage />,
})
