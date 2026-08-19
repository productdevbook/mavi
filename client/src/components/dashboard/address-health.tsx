import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Activity, Loader2 } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { Badge } from "@/components/ui/badge"
import type { Check } from "@api"

export function AddressHealth() {
  const { t } = useLingui()
  const [checks, setChecks] = React.useState<Check[] | null>(null)

  React.useEffect(() => {
    api("health.read")
      .then((health) => setChecks(health.checks))
      .catch((why: unknown) => {
        toast.error(said(why))
        setChecks([])
      })
  }, [])

  if (checks === null) {
    return (
      <div className="flex justify-center py-6">
        <Loader2 className="size-5 animate-spin text-muted-foreground" />
      </div>
    )
  }

  if (checks.length === 0) {
    return null
  }

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-border p-4">
      <div className="flex items-center gap-2">
        <Activity className="size-4 text-muted-foreground" />
        <h2 className="text-sm font-medium">{t`Health`}</h2>
      </div>

      <div className="flex flex-col divide-y divide-border">
        {checks.map((check) => (
          <div key={check.what} className="flex items-center justify-between py-2 text-sm">
            <span>{check.what}</span>
            <Badge variant={check.well ? "outline" : "destructive"}>
              {check.well ? t`Healthy` : t`Issue`}
            </Badge>
          </div>
        ))}
      </div>
    </section>
  )
}
