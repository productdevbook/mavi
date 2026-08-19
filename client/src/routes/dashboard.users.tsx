/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { PeoplePage } from "@/features/people/people-page"

export const Route = createFileRoute("/dashboard/users")({
  component: PeoplePage,
})
