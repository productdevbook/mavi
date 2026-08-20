import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Activity, Gauge } from "lucide-react"

import { every } from "@/lib/api"
import type { AnalyticsEvent } from "@api"
import { Figure, Panel } from "@/components/charts"
import {
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

export function PerformancePage() {
  const { t } = useLingui()
  const [events, setEvents] = React.useState<AnalyticsEvent[] | null>(null)

  React.useEffect(() => {
    let current = true

    every("analytics.events.list", { query: {} })
      .then((found) =>
        current &&
        setEvents(
          found.filter((event) =>
            ["lcp", "cls", "inp", "ttfb"].includes(event.event_name)
          )
        )
      )
      .catch(() => current && setEvents([]))

    return () => {
      current = false
    }
  }, [])

  if (events === null) {
    return <DashboardLoading />
  }

  const names: Record<string, string> = {
    lcp: t`Largest paint`,
    cls: t`Layout shift`,
    inp: t`Responds to input`,
    ttfb: t`First byte`,
  }

  const felts = measurements(events)
  const measured = events.length
  const uniquePages = new Set(events.map((event) => event.path)).size

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
                  {felts.map((f) => (
                    <tr
                      key={`${f.path}-${f.kind}`}
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

type Measurement = {
  path: string
  kind: string
  middle: number
  bad_end: number
  how_many: number
}

function measurements(events: AnalyticsEvent[]): Measurement[] {
  const groups = new Map<string, number[]>()

  for (const event of events) {
    const key = `${event.path}\u0000${event.event_name}`
    groups.set(key, [...(groups.get(key) ?? []), event.value])
  }

  return Array.from(groups.entries()).map(([key, values]) => {
    const [path, kind] = key.split("\u0000")
    const sorted = values.sort((a, b) => a - b)
    return {
      path,
      kind,
      middle: percentile(sorted, 0.5),
      bad_end: percentile(sorted, 0.95),
      how_many: sorted.length,
    }
  })
}

function percentile(values: number[], position: number): number {
  return values[Math.min(values.length - 1, Math.floor(values.length * position))]
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
