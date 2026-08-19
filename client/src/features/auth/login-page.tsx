import * as React from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import { Trans, useLingui } from "@lingui/react/macro"
import { Loader2 } from "lucide-react"

import { serverNextMessage, signIn } from "@/lib/server-next-auth"
import { AuthPageFrame } from "@/features/auth/auth-page-frame"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

/**
 * Signing in to this site.
 *
 * One site, one door: whichever address this was opened on decides which site
 * it is, and an account on another one is not an account here.
 */
export function LoginPage({ redirectTo }: { redirectTo?: string }) {
  const { t } = useLingui()
  const navigate = useNavigate()

  const [email, setEmail] = React.useState("")
  const [password, setPassword] = React.useState("")
  const [refused, setRefused] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  const ready = email.trim().length > 0 && password.length > 0 && !busy

  const submit = async () => {
    setBusy(true)
    setRefused("")

    try {
      await signIn(email.trim(), password)

      await navigate({ to: redirectTo ?? "/dashboard" })
    } catch (why) {
      setRefused(serverNextMessage(why))
      setBusy(false)
    }
  }

  return (
    <AuthPageFrame>
      <form
        onSubmit={(event) => {
          event.preventDefault()
          if (ready) void submit()
        }}
        className="flex flex-col gap-4"
      >
        <div>
          <h2 className="text-base font-semibold">
            <Trans>Sign in</Trans>
          </h2>
          <p className="text-sm text-muted-foreground">
            <Trans>Sign in to manage your site.</Trans>
          </p>
        </div>

        {refused && (
          <p className="rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {refused}
          </p>
        )}

        <div className="flex flex-col gap-1.5">
          <Label htmlFor="login-email">
            <Trans>Email</Trans>
          </Label>
          <Input
            id="login-email"
            type="email"
            autoComplete="username"
            autoFocus
            value={email}
            onChange={(event) => setEmail(event.target.value)}
          />
        </div>

        <div className="flex flex-col gap-1.5">
          <Label htmlFor="login-password">
            <Trans>Password</Trans>
          </Label>
          <Input
            id="login-password"
            type="password"
            autoComplete="current-password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
          />
        </div>

        <Button type="submit" disabled={!ready} className="w-full">
          {busy ? <Loader2 className="size-4 animate-spin" /> : t`Sign in`}
        </Button>

        <Link
          to="/forgotten"
          className="text-center text-xs text-muted-foreground hover:underline"
        >
          <Trans>I have forgotten my password</Trans>
        </Link>
      </form>
    </AuthPageFrame>
  )
}
