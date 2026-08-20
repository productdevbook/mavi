import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Rocket } from "lucide-react"
import { toast } from "sonner"

import { every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { DesignChange } from "@api"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  DashboardEmpty,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

export function PublishPage() {
  const { t } = useLingui()

  const [changes, setChanges] = React.useState<DesignChange[] | null>(null)
  const [showing, setShowing] = React.useState<string | null>(null)

  const load = React.useCallback(() => {
    every("design.changes.list")
      .then(setChanges)
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setChanges([])
      })
  }, [])

  React.useEffect(load, [load])

  const states: Record<string, string> = {
    writing: t`Writing`,
    building: t`Building`,
    ready: t`Ready to preview`,
    failed: t`Broken`,
    published: t`Live`,
  }

  return (
    <div className="flex max-w-3xl flex-col gap-5">
      <DashboardPageHeader
        title={t`Publish`}
        description={t`What has been published. A set of changes leaves what is live alone until published.`}
      />

      {changes === null ? (
        <DashboardLoading />
      ) : changes.length === 0 ? (
        <DashboardEmpty
          icon={Rocket}
          title={t`Nothing has been published yet.`}
          description={t`Build and publish a design change to put a version live.`}
        />
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {changes.map((change) => (
            <div key={change.id} className="px-4 py-3">
              <div className="flex flex-wrap items-center gap-3">
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-medium">{change.name}</p>
                  <p className="truncate font-mono text-xs text-muted-foreground">
                    {new Date(change.created_at).toLocaleString()}
                  </p>
                </div>

                <Badge
                  variant={change.state === "published" ? "default" : "secondary"}
                >
                  {states[change.state] ?? change.state}
                </Badge>

                {change.last_error && (
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

              {showing === change.id && change.last_error && (
                <pre className="mt-2 max-h-64 overflow-auto rounded-lg bg-muted px-3 py-2 text-xs">
                  {change.last_error}
                </pre>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
