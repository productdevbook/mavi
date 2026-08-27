/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { PluginsPage } from "@/features/plugins/plugins-page"

export const Route = createFileRoute("/dashboard/plugins")({
  component: PluginsPage,
})
