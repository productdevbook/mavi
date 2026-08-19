/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { RolesPage } from "@/features/people/roles-page"

export const Route = createFileRoute("/dashboard/roles")({
  component: RolesPage,
})
