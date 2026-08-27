/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const OrdersPage = lazyRoute(() =>
  import("@/features/shop/orders-page").then(({ OrdersPage }) => ({
    default: OrdersPage,
  })),
)

export const Route = createFileRoute("/dashboard/orders")({
  component: () => <OrdersPage />,
})
