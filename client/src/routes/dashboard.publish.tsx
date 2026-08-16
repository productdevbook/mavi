/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Rocket } from "lucide-react"
import { toast } from "sonner"

import { every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Change } from "@api"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"

export const Route = createFileRoute("/dashboard/publish")({
  component: PublishRoute,
})

function PublishRoute() {
  const { t } = useLingui()

  const [changes, setChanges] = React.useState<Change[] | null>(null)
  const [showing, setShowing] = React.useState<string | null>(null)

  const load = React.useCallback(() => {
    every("GET /api/design/changes")
      .then(setChanges)
      .catch((why: unknown) => {
        toast.error(said(why))
        setChanges([])
      })
  }, [])

  React.useEffect(load, [load])

  const states: Record<string, string> = {
    writing: t`Writing`,
    to_look_at: t`Ready to preview`,
    broken: t`Broken`,
    published: t`Live`,
  }

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <div>
        <h1 className="text-lg font-semibold">{t`Publish`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`What has been published. A set of changes leaves what is live alone until published.`}
        </p>
      </div>

      {changes === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : changes.length === 0 ? (
        <div className="rounded-xl border border-dashed border-border py-16 text-center">
          <Rocket className="mx-auto mb-3 size-8 text-muted-foreground" />
          <p className="text-sm text-muted-foreground">
            {t`Nothing has been published yet.`}
          </p>
        </div>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {changes.map((change) => (
            <div key={change.id} className="px-4 py-3">
              <div className="flex flex-wrap items-center gap-3">
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-medium">
                    {change.name}
                  </p>
                  <p className="truncate font-mono text-xs text-muted-foreground">
                    {new Date(change.created_at).toLocaleString()}
                  </p>
                </div>

                <Badge
                  variant={change.at === "published" ? "default" : "secondary"}
                >
                  {states[change.at] ?? change.at}
                </Badge>

                {change.went_wrong && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() =>
                      setShowing(showing === change.id ? null : change.id)
                    }
                  >
                    {t`What went wrong`}
                  </Button>
                )}
              </div>

              {showing === change.id && change.went_wrong && (
                <pre className="mt-2 max-h-64 overflow-auto rounded-lg bg-muted px-3 py-2 text-xs">
                  {change.went_wrong}
                </pre>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
