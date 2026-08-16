/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
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
 * One screen for three links: asking for a reset, choosing a password with
 * one that arrived, and proving an address changed elsewhere. An invitation
 * carries the same kind of link as a reset, so somebody just given an
 * account lands here too — but a link that only proves an address is not
 * asked to choose a password at all, so which of the two a token is for is
 * asked of the server before this shows either form.
 *
 * Asking for a reset says the same thing whether or not the address is one
 * this site knows: which addresses have accounts is not a question this
 * answers.
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

  // What a token with no meaning of its own turns out to be for: a proof
  // that only proves, or something this screen still has to ask a password
  // for. Known only once the proof has been tried, because the ticket does
  // not say what it was minted for until it is redeemed.
  const [proof, setProof] = React.useState<"checking" | "proved" | "elsewhere">(
    token ? "checking" : "elsewhere",
  )

  React.useEffect(() => {
    if (!token) {
      return
    }

    const link = token
    let live = true

    void (async () => {
      try {
        await api("POST /api/addresses", { body: { token: link } })
        if (live) setProof("proved")
      } catch {
        if (live) setProof("elsewhere")
      }
    })()

    return () => {
      live = false
    }
  }, [token])

  const ask = async () => {
    setBusy(true)
    setRefused("")

    try {
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
      await api("POST /api/passwords", {
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
            {token && proof === "checking" ? (
              <div className="flex justify-center py-4">
                <Loader2 className="size-5 animate-spin text-muted-foreground" />
              </div>
            ) : token && proof === "proved" ? (
              <div className="flex flex-col gap-2">
                <h2 className="text-base font-semibold">
                  <Trans>Your address is confirmed</Trans>
                </h2>
                <p className="text-sm text-muted-foreground">
                  <Trans>
                    Nothing else about your account changed. You can sign in
                    the way you always have.
                  </Trans>
                </p>
                <Button className="mt-2" render={<Link to="/login" />}>
                  <Trans>Go to sign in</Trans>
                </Button>
              </div>
            ) : token ? (
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
