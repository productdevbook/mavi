import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Activity, Gauge } from "lucide-react"

import { api } from "@/lib/v1"
import type { Felt } from "@api"
import { Figure, Panel } from "@/components/charts"
import {
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

export function PerformancePage() {
  const { t } = useLingui()
  const [felts, setFelts] = React.useState<Felt[] | null>(null)

  React.useEffect(() => {
    let current = true

    api("GET /api/analytics/felt")
      .then((found) => current && setFelts(found))
      .catch(() => current && setFelts([]))

    return () => {
      current = false
    }
  }, [])

  if (felts === null) {
    return <DashboardLoading />
  }

  const names: Record<string, string> = {
    lcp: t`Largest paint`,
    cls: t`Layout shift`,
    inp: t`Responds to input`,
    ttfb: t`First byte`,
  }

  const measured = felts.reduce((all, f) => all + f.how_many, 0)
  const uniquePages = new Set(felts.map((f) => f.path)).size

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`Performance`}
        description={t`See how quickly the published site responds for real visitors.`}
      />

      {felts.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          {t`Nothing measured yet. Numbers appear once real visitors reach the published site.`}
        </p>
      ) : (
        <>
          <div className="grid gap-4 sm:grid-cols-2">
            <Figure label={t`Measurements`} value={measured} icon={Gauge} />
            <Figure
              label={t`Pages measured`}
              value={uniquePages}
              icon={Activity}
            />
          </div>

          <Panel title={t`By page and metric`}>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b text-left text-xs text-muted-foreground">
                    <th className="py-2 pr-3 font-normal">{t`Page`}</th>
                    <th className="py-2 pr-3 font-normal">{t`Metric`}</th>
                    <th className="py-2 pr-3 font-normal">{t`Median`}</th>
                    <th className="py-2 pr-3 font-normal">{t`95th percentile`}</th>
                    <th className="py-2 font-normal">{t`Samples`}</th>
                  </tr>
                </thead>
                <tbody>
                  {felts.map((f, idx) => (
                    <tr
                      key={`${f.path}-${f.kind}-${idx}`}
                      className="border-b last:border-0"
                    >
                      <td className="max-w-[22rem] truncate py-2 pr-3 font-mono text-xs">
                        {f.path}
                      </td>
                      <td className="py-2 pr-3 text-xs uppercase">
                        {names[f.kind] ?? f.kind}
                      </td>
                      <td className="py-2 pr-3 tabular-nums">
                        {reading(f.kind, f.middle)}
                      </td>
                      <td className="py-2 pr-3 tabular-nums">
                        {reading(f.kind, f.bad_end)}
                      </td>
                      <td className="py-2 tabular-nums">{f.how_many}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Panel>
        </>
      )}
    </div>
  )
}

/**
 * A measurement as it is written about. Everything is milliseconds except
 * layout shift, which is a ratio the API keeps in hundredths.
 */
function reading(kind: string, value: number | null | undefined): string {
  if (value === null || value === undefined) {
    return "—"
  }

  return kind === "cls" ? (value / 100).toFixed(2) : `${value} ms`
}
