/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { ForgottenPage } from "@/features/auth/forgotten-page"

export const Route = createFileRoute("/forgotten")({
  validateSearch: (search: Record<string, unknown>): { token?: string } => ({
    token: typeof search.token === "string" ? search.token : undefined,
  }),
  component: ForgottenRoute,
})

function ForgottenRoute() {
  const { token } = Route.useSearch()

  return <ForgottenPage token={token} />
}
