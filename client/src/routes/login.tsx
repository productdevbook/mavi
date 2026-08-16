/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
import { Trans, useLingui } from "@lingui/react/macro"
import { Loader2 } from "lucide-react"

import { signIn } from "@/lib/v1-auth"
import { said } from "@/lib/v1-said"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

export const Route = createFileRoute("/login")({
  validateSearch: (search: Record<string, unknown>): { redirect?: string } => ({
    redirect: typeof search.redirect === "string" ? search.redirect : undefined,
  }),
  component: LoginRoute,
})

/**
 * Signing in to this site.
 *
 * One site, one door: whichever address this was opened on decides which site
 * it is, and an account on another one is not an account here.
 */
function LoginRoute() {
  const { t } = useLingui()
  const [providers, setProviders] = React.useState<
    { key: string; label: string }[]
  >([])
  const navigate = useNavigate()
  const { redirect: redirectTo } = Route.useSearch()

  const [email, setEmail] = React.useState("")
  const [password, setPassword] = React.useState("")
  const [code, setCode] = React.useState("")
  const [moment, setMoment] = React.useState<string | null>(null)
  const [wantsCode, setWantsCode] = React.useState(false)
  const [refused, setRefused] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  const ready = email.trim().length > 0 && password.length > 0 && !busy

  React.useEffect(() => {
    setProviders([])
  }, [])

  const startWith = async (_key: string) => {
    // Optional OAuth handler
  }

  const submit = async () => {
    setBusy(true)
    setRefused("")

    try {
      const answer = await signIn(email.trim(), password, code, moment ?? undefined)

      if (!answer.done) {
        setMoment(answer.moment)
        setWantsCode(true)
        setBusy(false)
        return
      }

      await navigate({ to: redirectTo ?? "/dashboard" })
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

              {wantsCode && (
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor="login-code">
                    <Trans>The six digits from your authenticator</Trans>
                  </Label>
                  <Input
                    id="login-code"
                    inputMode="numeric"
                    autoComplete="one-time-code"
                    autoFocus
                    value={code}
                    onChange={(event) => setCode(event.target.value)}
                  />
                </div>
              )}

              <Button type="submit" disabled={!ready} className="w-full">
                {busy ? <Loader2 className="size-4 animate-spin" /> : t`Sign in`}
              </Button>

              {providers.length > 0 && (
                <div className="flex flex-col gap-2 border-t border-border pt-4">
                  {providers.map((provider) => (
                    <Button
                      key={provider.key}
                      type="button"
                      variant="outline"
                      onClick={() => void startWith(provider.key)}
                    >
                      {t`Continue with ${provider.label}`}
                    </Button>
                  ))}
                </div>
              )}

              <Link
                to="/forgotten"
                className="text-center text-xs text-muted-foreground hover:underline"
              >
                <Trans>I have forgotten my password</Trans>
              </Link>
            </form>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
