/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const DesignPage = lazyRoute(() =>
  import("@/features/design/design-page").then(({ DesignPage }) => ({
    default: DesignPage,
  })),
)

export const Route = createFileRoute("/dashboard/design")({
  component: () => <DesignPage />,
})
