/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { StartTeachingPage } from "@/features/learning/start-teaching-page"

export const Route = createFileRoute("/dashboard/teaching/start")({
  component: StartTeachingPage,
})
