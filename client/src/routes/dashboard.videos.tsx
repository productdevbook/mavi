/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const VideosPage = lazyRoute(() =>
  import("@/features/media/videos-page").then(({ VideosPage }) => ({
    default: VideosPage,
  })),
)

export const Route = createFileRoute("/dashboard/videos")({
  component: () => <VideosPage />,
})
