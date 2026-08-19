/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { HomePage } from "@/features/dashboard/home-page"

export const Route = createFileRoute("/dashboard/")({
  component: HomePage,
})
