import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Download, KeyRound, Loader2, ShieldCheck, Upload } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { SecondStanding, SecondToSetUp, Settings } from "@api"
import { AddressHealth } from "@/components/dashboard/address-health"
import { DashboardPageHeader } from "@/components/dashboard/dashboard-page"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

/**
 * What this site is, and the two things about your own account that belong
 * nowhere else: the second factor, and a copy of what the site holds.
 */
export function SettingsPage() {
  const { t } = useLingui()

  const [site, setSite] = React.useState<Settings | null>(null)
  const [name, setName] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    api("GET /api/settings")
      .then((found) => {
        setSite(found)
        setName(found.name)
      })
      .catch((why: unknown) => {
        toast.error(said(why))
        setSite(null)
      })
  }, [])

  React.useEffect(load, [load])

  const rename = async () => {
    setBusy(true)

    try {
      await api("PATCH /api/settings", { body: { name: name.trim() } })
      load()
      toast.success(t`Saved`)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="flex max-w-2xl flex-col gap-8">
      <DashboardPageHeader
        title={t`This site`}
        description={t`What it is called and where it answers.`}
      />

      <section className="flex flex-col gap-3">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="site-name">{t`What it is called`}</Label>
          <Input
            id="site-name"
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
        </div>

        {site && (
          <p className="text-xs text-muted-foreground">{site.time_zone}</p>
        )}

        <Button
          className="self-start"
          disabled={!name.trim() || busy}
          onClick={() => void rename()}
        >
          {busy && <Loader2 className="animate-spin" />}
          {t`Save`}
        </Button>
      </section>

      <Working />

      <AddressHealth />

      <SecondFactor />

      <Providers />

      <ACopy />
    </div>
  )
}

/**
 * A second factor on your own account.
 */
function SecondFactor() {
  const { t } = useLingui()

  const [state, setState] = React.useState<SecondStanding | null>(null)
  const [secret, setSecret] = React.useState<SecondToSetUp | null>(null)
  const [code, setCode] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    api("GET /api/second")
      .then(setState)
      .catch((why: unknown) => {
        toast.error(said(why))
        setState(null)
      })
  }, [])

  React.useEffect(load, [load])

  const begin = async () => {
    setBusy(true)

    try {
      setSecret(await api("POST /api/second"))
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  const confirm = async () => {
    setBusy(true)

    try {
      await api("POST /api/second/confirm", {
        body: { code: code.trim() },
      })
      setSecret(null)
      setCode("")
      load()
      toast.success(t`On. You will be asked for the digits when you sign in.`)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  const off = async () => {
    setBusy(true)

    try {
      await api("DELETE /api/second", { body: { code: code.trim() } })
      setCode("")
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-border p-4">
      <div className="flex items-center gap-2">
        <ShieldCheck className="size-4 text-muted-foreground" />
        <h2 className="text-sm font-medium">{t`A second factor`}</h2>
        {state && (
          <Badge variant={state.confirmed ? "default" : "secondary"}>
            {state.confirmed ? t`On` : t`Off`}
          </Badge>
        )}
      </div>

      <p className="text-sm text-muted-foreground">
        {t`Six digits from an app on your phone, as well as your password. It is asked for when you sign in and nowhere else.`}
      </p>

      {state?.confirmed ? (
        <div className="flex flex-col gap-2">
          <Label htmlFor="factor-code">
            {t`The six digits from your app to turn it off`}
          </Label>
          <Input
            id="factor-code"
            inputMode="numeric"
            value={code}
            onChange={(event) => setCode(event.target.value)}
          />
          <Button
            variant="outline"
            className="self-start"
            disabled={!code.trim() || busy}
            onClick={() => void off()}
          >
            {busy && <Loader2 className="animate-spin" />}
            {t`Turn it off`}
          </Button>
        </div>
      ) : secret ? (
        <div className="flex flex-col gap-2">
          <p className="text-xs text-muted-foreground">
            {t`Add this to your authenticator, then type what it shows.`}
          </p>
          <code className="block overflow-x-auto rounded-md border border-border px-3 py-2 font-mono text-xs">
            {secret.typed_in}
          </code>

          <Label htmlFor="factor-code">{t`The six digits`}</Label>
          <Input
            id="factor-code"
            inputMode="numeric"
            value={code}
            onChange={(event) => setCode(event.target.value)}
          />

          <Button
            className="self-start"
            disabled={!code.trim() || busy}
            onClick={() => void confirm()}
          >
            {busy && <Loader2 className="animate-spin" />}
            {t`Turn it on`}
          </Button>
        </div>
      ) : (
        <Button
          variant="outline"
          className="self-start"
          disabled={busy}
          onClick={() => void begin()}
        >
          <KeyRound /> {t`Set one up`}
        </Button>
      )}
    </section>
  )
}

/**
 * Ways in other than a password.
 *
 * A site says where to send somebody and what to ask for; this machine keeps
 * the secret sealed and never hands it back. What comes back has to be an
 * address the site already knows: signing in with somebody else's Google
 * account is not a way into an account here.
 */
function Providers() {
  return null
}

/**
 * A copy of what the site holds, and reading one back.
 *
 * Languages, what things are filed under, and what has been written. Not the
 * media itself, not the accounts, not what a site keeps sealed — a file that
 * carried those would be a file that must never be lost.
 */
function ACopy() {
  const { t } = useLingui()
  const [busy, setBusy] = React.useState(false)
  const chooser = React.useRef<HTMLInputElement>(null)

  const take = async () => {
    setBusy(true)

    try {
      const bundle = await api("GET /api/portable")

      const url = URL.createObjectURL(
        new Blob([JSON.stringify(bundle, null, 2)], {
          type: "application/json",
        })
      )

      const link = document.createElement("a")

      link.href = url
      link.download = `site-${new Date().toISOString().slice(0, 10)}.json`
      link.click()

      URL.revokeObjectURL(url)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  const read = async (files: FileList | null) => {
    const file = files?.[0]

    if (!file) return

    setBusy(true)

    try {
      const bundle = JSON.parse(await file.text())
      await api("POST /api/portable", { body: bundle })

      toast.success(t`Read in.`)
    } catch (why) {
      toast.error(
        why instanceof SyntaxError
          ? t`That is not a copy of a site.`
          : said(why)
      )
    } finally {
      setBusy(false)

      if (chooser.current) {
        chooser.current.value = ""
      }
    }
  }

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-border p-4">
      <h2 className="text-sm font-medium">{t`A copy of what is written`}</h2>
      <p className="text-sm text-muted-foreground">
        {t`The languages, what things are filed under, and what has been written. Not the uploads, the accounts, or anything the site keeps sealed.`}
      </p>

      <div className="flex gap-2">
        <Button variant="outline" disabled={busy} onClick={() => void take()}>
          {busy ? <Loader2 className="animate-spin" /> : <Download />}
          {t`Take a copy`}
        </Button>

        <Button
          variant="outline"
          disabled={busy}
          onClick={() => chooser.current?.click()}
        >
          <Upload /> {t`Read one in`}
        </Button>

        <input
          ref={chooser}
          type="file"
          accept="application/json"
          className="hidden"
          onChange={(event) => void read(event.target.files)}
        />
      </div>
    </section>
  )
}

/**
 * Whether the things this site depends on are answering.
 *
 * Asked now rather than read from somewhere: what this says is the state of
 * the machine at the moment somebody looked, which is the only useful moment.
 */
function Working() {
  const { t } = useLingui()
  const [health, setHealth] = React.useState<{
    well: boolean
    checks: { what: string; well: boolean; detail: unknown }[]
  } | null>(null)

  React.useEffect(() => {
    api("GET /api/health")
      .then(setHealth)
      .catch((why: unknown) => {
        toast.error(said(why))
        setHealth(null)
      })
  }, [])

  if (!health) {
    return null
  }

  return (
    <section className="flex flex-col gap-2 rounded-xl border border-border p-4">
      <div className="flex items-center gap-2">
        <h2 className="text-sm font-medium">{t`Is everything answering`}</h2>
        <Badge variant={health.well ? "default" : "secondary"}>
          {health.well ? t`Yes` : t`Not everything`}
        </Badge>
      </div>

      <div className="flex flex-col divide-y divide-border">
        {health.checks.map((check) => (
          <div key={check.what} className="flex items-center gap-3 py-2">
            <span className="min-w-0 flex-1 truncate text-sm">
              {check.what}
            </span>
            <Badge variant={check.well ? "default" : "secondary"}>
              {check.well ? t`Answering` : t`Not answering`}
            </Badge>
          </div>
        ))}
      </div>
    </section>
  )
}
