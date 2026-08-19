/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { MediaPage } from "@/features/media/media-page"

export const Route = createFileRoute("/dashboard/media")({
  component: MediaPage,
})
