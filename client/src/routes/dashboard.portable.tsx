/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const PortablePage = lazyRoute(() =>
  import("@/features/portability/portable-page").then(({ PortablePage }) => ({
    default: PortablePage,
  })),
)

export const Route = createFileRoute("/dashboard/portable")({
  component: () => <PortablePage />,
})
