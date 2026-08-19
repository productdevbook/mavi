import * as React from "react"
import { useNavigate } from "@tanstack/react-router"
import { Trans, useLingui } from "@lingui/react/macro"
import { Loader2 } from "lucide-react"

import { api } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import { AuthPageFrame } from "@/features/auth/auth-page-frame"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

/**
 * One screen for two steps: asking for a reset and choosing a password with
 * the one-time token that arrived. Email verification is a separate API
 * operation and is not guessed from a token on this screen.
 *
 * Asking for a reset says the same thing whether or not the address is one
 * this site knows: which addresses have accounts is not a question this
 * answers.
 */
export function ForgottenPage({ token }: { token?: string }) {
  const { t } = useLingui()
  const navigate = useNavigate()

  const [email, setEmail] = React.useState("")
  const [password, setPassword] = React.useState("")
  const [asked, setAsked] = React.useState(false)
  const [refused, setRefused] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  const ask = async () => {
    setBusy(true)
    setRefused("")

    try {
      await api("auth.password_reset.request", {
        body: { email: email.trim() },
      })
      setAsked(true)
    } catch (why) {
      setRefused(apiMessage(why))
    } finally {
      setBusy(false)
    }
  }

  const choose = async () => {
    setBusy(true)
    setRefused("")

    try {
      await api("auth.password_reset.redeem", {
        body: { token: token ?? "", password },
      })
      await navigate({ to: "/login" })
    } catch (why) {
      setRefused(apiMessage(why))
      setBusy(false)
    }
  }

  return (
    <AuthPageFrame>
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
                At least twelve characters. Everything else that was open closes
                when you do this.
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
              If that address has an account here, a link is on its way. It
              works once and stops working after a day.
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
              <Trans>We will send a link to the address on the account.</Trans>
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
    </AuthPageFrame>
  )
}
