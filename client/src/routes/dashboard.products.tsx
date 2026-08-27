/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const ProductsPage = lazyRoute(() =>
  import("@/features/shop/products-page").then(({ ProductsPage }) => ({
    default: ProductsPage,
  })),
)

export const Route = createFileRoute("/dashboard/products")({
  component: () => <ProductsPage />,
})
