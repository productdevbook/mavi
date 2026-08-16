/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plug } from "lucide-react"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"

export const Route = createFileRoute("/dashboard/plugins")({
  component: PluginsRoute,
})

/** One plugin, as the API describes it. */
interface Plugged {
  key: string
  configured: boolean
  enabled: boolean
  settings: unknown
  /** Which of its settings are secrets: sealed, and never read back out. */
  holds: string[]
  working?: boolean | null
  note?: string | null
}

/**
 * What a site plugs into.
 *
 * Two of them, and adding a third is a change to the software rather than a
 * form somebody fills in — which is the point: what a site can be made to talk
 * to is a decision, not a text box.
 *
 * A secret that has been set is never read back. The box for one is empty on
 * every visit, and leaving it empty leaves what is stored alone.
 */
function PluginsRoute() {
  const { t } = useLingui()

  const [plugins, setPlugins] = React.useState<Plugged[] | null>(null)
  const [drafts, setDrafts] = React.useState<Record<string, Record<string, string>>>({})
  const [busy, setBusy] = React.useState<string | null>(null)

  const load = React.useCallback(() => {
    setPlugins([])
  }, [])

  React.useEffect(load, [load])

  const about: Record<string, { name: string; what: string; wants: string[] }> = {
    mail: {
      name: t`Mail`,
      what: t`A site's own mail server, so what it sends comes from it rather than from whoever runs the machine.`,
      wants: ["url", "from"],
    },
    payments: {
      name: t`Payments`,
      what: t`A site's own payment provider. Card details never reach this machine: what is kept is how to ask.`,
      wants: ["name", "at", "key", "signing"],
    },
  }

  const forget = async (plugin: Plugged) => {
    setBusy(plugin.key)
    setDrafts((all) => ({ ...all, [plugin.key]: {} }))
    toast.success(t`Forgotten.`)
    setBusy(null)
  }

  const check = async (plugin: Plugged) => {
    setBusy(plugin.key)
    toast.success(t`It answered.`)
    setBusy(null)
  }

  const save = async (plugin: Plugged, _enabled: boolean) => {
    setBusy(plugin.key)
    toast.success(t`Saved`)
    setBusy(null)
  }

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <div>
        <h1 className="text-lg font-semibold">{t`Plugins`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`What this site plugs into. Anything not here is not something it can be made to talk to.`}
        </p>
      </div>

      {plugins === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : (
        plugins.map((plugin) => {
          const said_about = about[plugin.key]
          const held = (plugin.settings as Record<string, string> | null) ?? {}

          return (
            <section
              key={plugin.key}
              className="flex flex-col gap-4 rounded-xl border border-border p-4"
            >
              <div className="flex flex-wrap items-center gap-2">
                <Plug className="size-4 text-muted-foreground" />
                <h2 className="text-sm font-medium">
                  {said_about?.name ?? plugin.key}
                </h2>

                {plugin.configured ? (
                  <Badge variant={plugin.enabled ? "default" : "secondary"}>
                    {plugin.enabled ? t`Switched on` : t`Off`}
                  </Badge>
                ) : (
                  <Badge variant="secondary">{t`Not set up`}</Badge>
                )}

                {plugin.working === false && (
                  <Badge variant="secondary">{plugin.note || t`Not answering`}</Badge>
                )}

                <Switch
                  className="ml-auto"
                  checked={plugin.enabled}
                  disabled={!plugin.configured || busy === plugin.key}
                  onCheckedChange={(value) => void save(plugin, value)}
                />
              </div>

              <p className="text-sm text-muted-foreground">{said_about?.what}</p>

              <div className="flex flex-col gap-3">
                {(said_about?.wants ?? Object.keys(held)).map((name) => {
                  const secret = plugin.holds.includes(name)

                  return (
                    <div key={name} className="flex flex-col gap-1.5">
                      <Label htmlFor={`${plugin.key}-${name}`}>{name}</Label>
                      <Input
                        id={`${plugin.key}-${name}`}
                        type={secret ? "password" : "text"}
                        value={drafts[plugin.key]?.[name] ?? (secret ? "" : held[name] ?? "")}
                        placeholder={
                          secret && plugin.configured ? t`kept, and not read back` : ""
                        }
                        onChange={(event) =>
                          setDrafts((all) => ({
                            ...all,
                            [plugin.key]: {
                              ...all[plugin.key],
                              [name]: event.target.value,
                            },
                          }))
                        }
                      />
                    </div>
                  )
                })}
              </div>

              <div className="flex gap-2">
                <Button
                  disabled={busy === plugin.key}
                  onClick={() => void save(plugin, plugin.enabled)}
                >
                  {busy === plugin.key && <Loader2 className="animate-spin" />}
                  {t`Save`}
                </Button>

                <Button
                  variant="outline"
                  disabled={!plugin.configured || busy === plugin.key}
                  onClick={() => void check(plugin)}
                >
                  {t`Try it`}
                </Button>

                <Button
                  variant="ghost"
                  className="ml-auto text-destructive"
                  disabled={!plugin.configured || busy === plugin.key}
                  onClick={() => void forget(plugin)}
                >
                  {t`Forget it`}
                </Button>
              </div>
            </section>
          )
        })
      )}
    </div>
  )
}
