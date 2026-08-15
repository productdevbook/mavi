/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { Trans } from "@lingui/react/macro"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"

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
 *
 * The code is traded for a session here rather than by the provider, because a
 * cookie can only be set for the host the browser is talking to — and it is
 * traded before anything renders, so the address with the code in it is
 * replaced rather than left in history.
 */
function ComingBackRoute() {
  const navigate = useNavigate()
  const { provider } = Route.useParams()
  const { code, state } = Route.useSearch()
  const [refused, setRefused] = React.useState("")

  React.useEffect(() => {
    if (!code || !state) {
      void navigate({ to: "/login" })
      return
    }

    api("POST /api/auth/oauth/{key}/callback", {
      path: { key: provider },
      body: {
        code,
        state,
        redirect_uri: `${window.location.origin}/oauth/${provider}`,
      },
    })
      .then((arrived) =>
        navigate({ to: arrived.redirect || "/dashboard", replace: true }),
      )
      .catch((why: unknown) => setRefused(said(why)))
  }, [code, state, provider, navigate])

  return (
    <div className="flex min-h-svh items-center justify-center px-4 text-sm text-muted-foreground">
      {refused || <Trans>Signing you in…</Trans>}
    </div>
  )
}
