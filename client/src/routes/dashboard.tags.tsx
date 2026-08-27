/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const TagsPage = lazyRoute(() =>
  import("@/features/taxonomy/tags-page").then(({ TagsPage }) => ({
    default: TagsPage,
  })),
)

export const Route = createFileRoute("/dashboard/tags")({
  component: () => <TagsPage />,
})
