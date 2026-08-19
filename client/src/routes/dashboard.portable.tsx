/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { PortablePage } from "@/features/portability/portable-page"

export const Route = createFileRoute("/dashboard/portable")({
  component: PortablePage,
})
