/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { LanguagesPage } from "@/features/content/languages-page"

export const Route = createFileRoute("/dashboard/languages")({
  component: LanguagesPage,
})
