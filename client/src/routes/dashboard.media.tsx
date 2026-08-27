/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const MediaPage = lazyRoute(() =>
  import("@/features/media/media-page").then(({ MediaPage }) => ({
    default: MediaPage,
  })),
)

export const Route = createFileRoute("/dashboard/media")({
  component: () => <MediaPage />,
})
