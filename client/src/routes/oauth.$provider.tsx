/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { Trans } from "@lingui/react/macro"

export const Route = createFileRoute("/oauth/$provider")({
  validateSearch: (
    search: Record<string, unknown>,
  ): { code?: string; state?: string } => ({
    code: typeof search.code === "string" ? search.code : undefined,
    state: typeof search.state === "string" ? search.state : undefined,
  }),
  component: ComingBackRoute,
})

/**
 * Coming back from whoever was asked to say who this is.
 */
function ComingBackRoute() {
  const navigate = useNavigate()
  const { provider } = Route.useParams()
  const { code, state } = Route.useSearch()

  React.useEffect(() => {
    if (!code || !state) {
      void navigate({ to: "/login" })
      return
    }

    void navigate({ to: "/dashboard", replace: true })
  }, [code, state, provider, navigate])

  return (
    <div className="flex min-h-svh items-center justify-center px-4 text-sm text-muted-foreground">
      <Trans>Signing you in…</Trans>
    </div>
  )
}
