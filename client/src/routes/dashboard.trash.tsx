/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const TrashPage = lazyRoute(() =>
  import("@/features/governance/trash-page").then(({ TrashPage }) => ({
    default: TrashPage,
  })),
)

export const Route = createFileRoute("/dashboard/trash")({
  component: () => <TrashPage />,
})
