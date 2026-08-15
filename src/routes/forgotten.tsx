/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { Trans, useLingui } from "@lingui/react/macro"
import { Loader2 } from "lucide-react"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

export const Route = createFileRoute("/forgotten")({
  validateSearch: (search: Record<string, unknown>): { token?: string } => ({
    token: typeof search.token === "string" ? search.token : undefined,
  }),
  component: ForgottenRoute,
})

/**
 * A password nobody remembers, and the link that replaces it.
 *
 * One screen for both halves: asking for the link, and choosing a password
 * with the one that arrived. An invitation carries the same kind of link, so
 * somebody who has just been given an account lands here too.
 *
 * Asking says the same thing whether or not the address is one this site
 * knows: which addresses have accounts is not a question this answers.
 */
function ForgottenRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const { token } = Route.useSearch()

  const [email, setEmail] = React.useState("")
  const [password, setPassword] = React.useState("")
  const [asked, setAsked] = React.useState(false)
  const [refused, setRefused] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  const ask = async () => {
    setBusy(true)
    setRefused("")

    try {
      await api("POST /api/auth/reset", { body: { email: email.trim() } })
      setAsked(true)
    } catch (why) {
      setRefused(said(why))
    } finally {
      setBusy(false)
    }
  }

  const choose = async () => {
    setBusy(true)
    setRefused("")

    try {
      await api("POST /api/auth/password", {
        body: { token: token ?? "", password },
      })
      await navigate({ to: "/login" })
    } catch (why) {
      setRefused(said(why))
      setBusy(false)
    }
  }

  return (
    <div className="flex min-h-svh items-center justify-center bg-muted/40 p-4">
      <div className="w-full max-w-sm">
        <div className="mb-6 flex flex-col items-center gap-2">
          <span className="flex size-10 items-center justify-center rounded-xl bg-primary text-lg font-bold text-primary-foreground">
            M
          </span>
          <h1 className="text-lg font-semibold">Mavi CMS</h1>
        </div>

        <Card>
          <CardContent className="pt-6">
            {token ? (
              <form
                className="flex flex-col gap-4"
                onSubmit={(event) => {
                  event.preventDefault()
                  if (password.length >= 12) void choose()
                }}
              >
                <div>
                  <h2 className="text-base font-semibold">
                    <Trans>Choose a password</Trans>
                  </h2>
                  <p className="text-sm text-muted-foreground">
                    <Trans>
                      At least twelve characters. Everything else that was open
                      closes when you do this.
                    </Trans>
                  </p>
                </div>

                {refused && (
                  <p className="rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">
                    {refused}
                  </p>
                )}

                <div className="flex flex-col gap-1.5">
                  <Label htmlFor="new-password">
                    <Trans>Password</Trans>
                  </Label>
                  <Input
                    id="new-password"
                    type="password"
                    autoComplete="new-password"
                    autoFocus
                    value={password}
                    onChange={(event) => setPassword(event.target.value)}
                  />
                </div>

                <Button type="submit" disabled={password.length < 12 || busy}>
                  {busy ? <Loader2 className="size-4 animate-spin" /> : t`Save it`}
                </Button>
              </form>
            ) : asked ? (
              <div className="flex flex-col gap-2">
                <h2 className="text-base font-semibold">
                  <Trans>Look in your email</Trans>
                </h2>
                <p className="text-sm text-muted-foreground">
                  <Trans>
                    If that address has an account here, a link is on its way.
                    It works once and stops working after a day.
                  </Trans>
                </p>
              </div>
            ) : (
              <form
                className="flex flex-col gap-4"
                onSubmit={(event) => {
                  event.preventDefault()
                  if (email.trim()) void ask()
                }}
              >
                <div>
                  <h2 className="text-base font-semibold">
                    <Trans>A new password</Trans>
                  </h2>
                  <p className="text-sm text-muted-foreground">
                    <Trans>
                      We will send a link to the address on the account.
                    </Trans>
                  </p>
                </div>

                {refused && (
                  <p className="rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">
                    {refused}
                  </p>
                )}

                <div className="flex flex-col gap-1.5">
                  <Label htmlFor="forgotten-email">
                    <Trans>Email</Trans>
                  </Label>
                  <Input
                    id="forgotten-email"
                    type="email"
                    autoComplete="username"
                    autoFocus
                    value={email}
                    onChange={(event) => setEmail(event.target.value)}
                  />
                </div>

                <Button type="submit" disabled={!email.trim() || busy}>
                  {busy ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : (
                    t`Send the link`
                  )}
                </Button>
              </form>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
