import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Activity, Loader2 } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import { Badge } from "@/components/ui/badge"
import type { RuntimeManifest } from "@api"

export function AddressHealth() {
  const { t } = useLingui()
  const [manifest, setManifest] = React.useState<RuntimeManifest | null>(null)

  React.useEffect(() => {
    api("runtime.manifest.read")
      .then(setManifest)
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setManifest(null)
      })
  }, [])

  if (manifest === null) {
    return (
      <div className="flex justify-center py-6">
        <Loader2 className="size-5 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-border p-4">
      <div className="flex items-center gap-2">
        <Activity className="size-4 text-muted-foreground" />
        <h2 className="text-sm font-medium">{t`Health`}</h2>
      </div>

      <div className="flex flex-col divide-y divide-border">
        <div className="flex items-center justify-between py-2 text-sm">
          <span>{t`Runtime ${manifest.runtime_mode}`}</span>
          <Badge variant="outline">{t`Healthy`}</Badge>
        </div>
        <div className="flex items-center justify-between py-2 text-sm">
          <span>{t`Release ${manifest.release}`}</span>
          <span className="font-mono text-xs text-muted-foreground">
            {manifest.api_contract_hash.slice(0, 12)}
          </span>
        </div>
      </div>
    </section>
  )
}
