/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { Trans, useLingui } from "@lingui/react/macro"
import { Loader2 } from "lucide-react"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

export const Route = createFileRoute("/setup")({
  loader: async () => {
    const site = await api("GET /api/open/site").catch(() => null)

    if (site) {
      throw redirect({ to: "/dashboard" })
    }

    return null
  },
  component: SetupRoute,
})

/**
 * Setting the machine up: one account, once — and the one site that comes
 * with it, made in the same request.
 *
 * There is nothing else to ask. The database is where the machine was told it
 * is before it started, the site's address is whatever this was reached on,
 * and the one thing nobody else can do is be the first person — which is why
 * this is the only change in the whole API that asks for no grant at all.
 */
function SetupRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()

  const [siteName, setSiteName] = React.useState("")
  const [name, setName] = React.useState("")
  const [email, setEmail] = React.useState("")
  const [password, setPassword] = React.useState("")
  const [refused, setRefused] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  const ready =
    name.trim().length > 0 &&
    email.trim().length > 0 &&
    password.length >= 12 &&
    !busy

  const submit = async () => {
    setBusy(true)
    setRefused("")

    try {
      await api("POST /api/setup", {
        body: {
          site: siteName.trim() || "Mavi CMS",
          email: email.trim(),
          name: name.trim(),
          password,
        },
      })
      await navigate({ to: "/login" })
    } catch (why) {
      setRefused(said(why))
      setBusy(false)
    }
  }

  return (
    <div className="flex min-h-svh items-center justify-center bg-muted/40 p-4">
      <div className="w-full max-w-md">
        <div className="mb-6 flex flex-col items-center gap-2">
          <span className="flex size-10 items-center justify-center rounded-xl bg-primary text-lg font-bold text-primary-foreground">
            M
          </span>
          <h1 className="text-lg font-semibold">Mavi CMS</h1>
        </div>

        <Card>
          <CardContent className="pt-6">
            <form
              className="flex flex-col gap-4"
              onSubmit={(event) => {
                event.preventDefault()
                if (ready) void submit()
              }}
            >
              <div>
                <h2 className="text-base font-semibold">
                  <Trans>The first account</Trans>
                </h2>
                <p className="text-sm text-muted-foreground">
                  <Trans>
                    This is the account that runs the machine. Everybody else is
                    invited from inside, and this screen never appears again.
                  </Trans>
                </p>
              </div>

              {refused && (
                <p className="rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">
                  {refused}
                </p>
              )}

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="setup-site">
                  <Trans>Site name</Trans>
                </Label>
                <Input
                  id="setup-site"
                  placeholder="My Site"
                  value={siteName}
                  onChange={(event) => setSiteName(event.target.value)}
                />
              </div>

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="setup-name">
                  <Trans>Your name</Trans>
                </Label>
                <Input
                  id="setup-name"
                  autoFocus
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                />
              </div>

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="setup-email">
                  <Trans>Email</Trans>
                </Label>
                <Input
                  id="setup-email"
                  type="email"
                  autoComplete="username"
                  value={email}
                  onChange={(event) => setEmail(event.target.value)}
                />
              </div>

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="setup-password">
                  <Trans>Password</Trans>
                </Label>
                <Input
                  id="setup-password"
                  type="password"
                  autoComplete="new-password"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                />
                <p className="text-xs text-muted-foreground">
                  {t`At least twelve characters.`}
                </p>
              </div>

              <Button type="submit" disabled={!ready} className="w-full">
                {busy ? <Loader2 className="size-4 animate-spin" /> : t`Set up`}
              </Button>
            </form>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
