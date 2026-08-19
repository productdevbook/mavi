/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { OrdersPage } from "@/features/shop/orders-page"

export const Route = createFileRoute("/dashboard/orders")({
  component: OrdersPage,
})
