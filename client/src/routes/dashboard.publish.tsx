/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const PublishPage = lazyRoute(() =>
  import("@/features/design/publish-page").then(({ PublishPage }) => ({
    default: PublishPage,
  })),
)

export const Route = createFileRoute("/dashboard/publish")({
  component: () => <PublishPage />,
})
