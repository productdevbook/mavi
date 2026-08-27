import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Check, Loader2, Puzzle, Power } from "lucide-react"
import { toast } from "sonner"

import {
  DashboardError,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { apiMessage } from "@/lib/auth"
import { isPluginId, usePlugins } from "@/lib/plugins"

const pluginNames: Record<string, string> = {
  core: "Core",
  writing: "Writing",
  commerce: "Commerce",
  learning: "Learning",
  forms: "Forms",
  messaging: "Messaging",
  automation: "Automation",
  boards: "Boards",
  analytics: "Analytics",
  governance: "Governance",
}

const pluginDescriptions: Record<string, string> = {
  core: "Authentication, people, settings and plugin management.",
  writing: "Posts, pages, media, taxonomy, design and publishing.",
  commerce: "Products, orders, coupons and fulfilment workflows.",
  learning: "Courses, lessons, students and protected video access.",
  forms: "Forms and submissions.",
  messaging: "Mail, lists, letters and deliveries.",
  automation: "Flows and durable workflow runs.",
  boards: "Boards, lists, cards and collaboration.",
  analytics: "Visitors, performance and usage reporting.",
  governance: "Audit, trash and portable import/export.",
}

/** The owner-facing registry for compiled feature packages. */
export function PluginsPage() {
  const { t } = useLingui()
  const { records, refresh, enable, disable, loading, error } = usePlugins()
  const [pending, setPending] = React.useState<string | null>(null)

  React.useEffect(() => {
    void refresh(true).catch(() => undefined)
  }, [refresh])

  const toggle = async (id: string, enabled: boolean) => {
    if (!isPluginId(id)) return
    setPending(id)
    try {
      if (enabled) {
        await enable(id)
        toast.success(t`Plugin enabled.`)
      } else {
        await disable(id)
        toast.success(t`Plugin disabled.`)
      }
    } catch (why) {
      toast.error(apiMessage(why))
    } finally {
      setPending(null)
    }
  }

  return (
    <div className="flex max-w-4xl flex-col gap-6">
      <DashboardPageHeader
        title={t`Plugins`}
        description={t`Turn compiled Mavi features on for this site. Disabling a plugin hides its routes and navigation without deleting its data.`}
      />

      {records === null && !error ? <DashboardLoading /> : null}
      {error && records?.length === 0 ? (
        <DashboardError message={t`The plugin catalog could not be read just now.`} />
      ) : null}

      {records ? (
        <div className="grid gap-4 md:grid-cols-2">
          {records.map((plugin) => {
            const isCore = plugin.id === "core"
            const busy = pending === plugin.id || loading
            return (
              <Card key={plugin.id} size="sm">
                <CardHeader className="border-b">
                  <div className="flex items-start justify-between gap-3">
                    <div className="flex min-w-0 items-start gap-3">
                      <span className="mt-0.5 rounded-lg bg-muted p-2">
                        <Puzzle className="size-4" />
                      </span>
                      <div className="min-w-0">
                        <CardTitle>{pluginNames[plugin.id] ?? plugin.id}</CardTitle>
                        <p className="mt-1 text-xs text-muted-foreground">
                          {pluginDescriptions[plugin.id] ?? t`Compiled Mavi feature package.`}
                        </p>
                      </div>
                    </div>
                    <Badge variant={plugin.enabled ? "default" : "outline"}>
                      {plugin.enabled ? <Check /> : null}
                      {plugin.enabled ? t`Enabled` : t`Disabled`}
                    </Badge>
                  </div>
                </CardHeader>
                <CardContent className="flex items-center justify-between gap-3 pt-4">
                  <div className="text-xs text-muted-foreground">
                    <p>{t`Version ${plugin.version}`}</p>
                    {plugin.dependencies.length > 0 ? (
                      <p>
                        {t`Needs ${plugin.dependencies
                          .map((dependency) => pluginNames[dependency] ?? dependency)
                          .join(", ")}`}
                      </p>
                    ) : null}
                  </div>
                  {isCore ? (
                    <Badge variant="secondary">{t`Always on`}</Badge>
                  ) : (
                    <Button
                      size="sm"
                      variant={plugin.enabled ? "outline" : "default"}
                      disabled={busy}
                      onClick={() => void toggle(plugin.id, !plugin.enabled)}
                    >
                      {busy ? <Loader2 className="animate-spin" /> : <Power />}
                      {plugin.enabled ? t`Disable` : t`Enable`}
                    </Button>
                  )}
                </CardContent>
              </Card>
            )
          })}
        </div>
      ) : null}
    </div>
  )
}
