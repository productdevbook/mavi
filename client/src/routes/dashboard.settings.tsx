/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { SettingsPage } from "@/features/settings/settings-page"

export const Route = createFileRoute("/dashboard/settings")({
  component: SettingsPage,
})
