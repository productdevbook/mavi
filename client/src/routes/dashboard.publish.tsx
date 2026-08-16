/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Rocket, X } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Publish } from "@api"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"

export const Route = createFileRoute("/dashboard/publish")({
  component: PublishRoute,
})

/** How often to look again while a build is running. */
const WATCH_INTERVAL = 3000

/**
 * Every publish this site has made.
 *
 * What to publish and whether to look at it first is decided on the design
 * screen; this is the record of what happened — how long each took, what is
 * live now, and what a failed one said.
 */
function PublishRoute() {
  const { t } = useLingui()

  const [publishes, setPublishes] = React.useState<Publish[] | null>(null)
  const [showing, setShowing] = React.useState<string | null>(null)

  const load = React.useCallback(() => {
    every("GET /api/design/publishes")
      .then(setPublishes)
      .catch((why: unknown) => {
        toast.error(said(why))
        setPublishes([])
      })
  }, [])

  React.useEffect(load, [load])

  const running = (publishes ?? []).some(
    (one) => one.state === "queued" || one.state === "building",
  )

  React.useEffect(() => {
    if (!running) return

    const timer = window.setInterval(load, WATCH_INTERVAL)

    return () => window.clearInterval(timer)
  }, [running, load])

  const stop = async (publish: Publish) => {
    try {
      await api("POST /api/design/publishes/{id}/cancel", {
        path: { id: publish.id },
      })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const states: Record<string, string> = {
    queued: t`Waiting`,
    building: t`Building`,
    live: t`Live`,
    previewed: t`Looked at`,
    failed: t`Did not build`,
    cancelled: t`Stopped`,
  }

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <div>
        <h1 className="text-lg font-semibold">{t`Publish`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`What has been published, and what it cost in seconds. A build that fails leaves what is live alone.`}
        </p>
      </div>

      {publishes === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : publishes.length === 0 ? (
        <div className="rounded-xl border border-dashed border-border py-16 text-center">
          <Rocket className="mx-auto mb-3 size-8 text-muted-foreground" />
          <p className="text-sm text-muted-foreground">
            {t`Nothing has been published yet.`}
          </p>
        </div>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {publishes.map((publish) => (
            <div key={publish.id} className="px-4 py-3">
              <div className="flex flex-wrap items-center gap-3">
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-medium">
                    {new Date(publish.created_at).toLocaleString()}
                  </p>
                  <p className="truncate font-mono text-xs text-muted-foreground">
                    {publish.branch}
                    {publish.seconds !== null && publish.seconds !== undefined
                      ? ` · ${publish.seconds}s`
                      : ""}
                  </p>
                </div>

                <Badge
                  variant={publish.state === "live" ? "default" : "secondary"}
                >
                  {states[publish.state] ?? publish.state}
                </Badge>

                {(publish.state === "queued" ||
                  publish.state === "building") && (
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t`Stop it`}
                    onClick={() => void stop(publish)}
                  >
                    <X />
                  </Button>
                )}

                {publish.log && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() =>
                      setShowing(showing === publish.id ? null : publish.id)
                    }
                  >
                    {t`What it said`}
                  </Button>
                )}
              </div>

              {showing === publish.id && publish.log && (
                <pre className="mt-2 max-h-64 overflow-auto rounded-lg bg-muted px-3 py-2 text-xs">
                  {publish.log}
                </pre>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
