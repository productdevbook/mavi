/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { PublishPage } from "@/features/design/publish-page"

export const Route = createFileRoute("/dashboard/publish")({
  component: PublishPage,
})
