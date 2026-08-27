/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const ContentTypesPage = lazyRoute(() =>
  import("@/features/content/content-types-page").then(({ ContentTypesPage }) => ({
    default: ContentTypesPage,
  })),
)

export const Route = createFileRoute("/dashboard/content-types")({
  component: () => <ContentTypesPage />,
})
