import * as React from "react"
import { useNavigate } from "@tanstack/react-router"
import { Trans, useLingui } from "@lingui/react/macro"
import { Loader2 } from "lucide-react"

import { nextApi } from "@/lib/server-next"
import { serverNextMessage } from "@/lib/server-next-auth"
import { AuthPageFrame } from "@/features/auth/auth-page-frame"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

/**
 * Setting the machine up: one account, once — and the one site that comes
 * with it, made in the same request.
 *
 * There is nothing else to ask. The database is where the machine was told it
 * is before it started, the site's address is whatever this was reached on,
 * and the one thing nobody else can do is be the first person — which is why
 * this is the only change in the whole API that asks for no grant at all.
 */
export function SetupPage() {
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
      await nextApi("setup.initialize", {
        body: {
          site_name: siteName.trim() || "Mavi CMS",
          email: email.trim(),
          name: name.trim(),
          password,
        },
      })
      await navigate({ to: "/login" })
    } catch (why) {
      setRefused(serverNextMessage(why))
      setBusy(false)
    }
  }

  return (
    <AuthPageFrame wide>
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
    </AuthPageFrame>
  )
}
