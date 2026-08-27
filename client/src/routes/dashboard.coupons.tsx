/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const CouponsPage = lazyRoute(() =>
  import("@/features/shop/coupons-page").then(({ CouponsPage }) => ({
    default: CouponsPage,
  })),
)

export const Route = createFileRoute("/dashboard/coupons")({
  component: () => <CouponsPage />,
})
