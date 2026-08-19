/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { CouponsPage } from "@/features/shop/coupons-page"

export const Route = createFileRoute("/dashboard/coupons")({
  component: CouponsPage,
})
