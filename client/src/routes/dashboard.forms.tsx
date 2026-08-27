/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const FormsPage = lazyRoute(() =>
  import("@/features/forms/forms-page").then(({ FormsPage }) => ({
    default: FormsPage,
  })),
)

export const Route = createFileRoute("/dashboard/forms")({
  component: () => <FormsPage />,
})
