/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { VideosPage } from "@/features/media/videos-page"

export const Route = createFileRoute("/dashboard/videos")({
  component: VideosPage,
})
