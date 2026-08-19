/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { ProductsPage } from "@/features/shop/products-page"

export const Route = createFileRoute("/dashboard/products")({
  component: ProductsPage,
})
