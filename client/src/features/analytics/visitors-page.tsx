import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Eye, Users } from "lucide-react"

import { api } from "@/lib/v1"
import type { Read } from "@api"
import { Bars, Curve, Figure, Panel } from "@/components/charts"
import {
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

/**
 * Who has been reading, counted where the pages are served.
 *
 * No script runs on the site and nothing is kept that says who anybody was:
 * a day's salt turns an address into a mark, the mark says whether this day
 * has seen them before, and the mark goes when the day is over. What is left
 * is how many, and which pages.
 */
export function VisitorsPage() {
  const { t } = useLingui()
  const [reads, setReads] = React.useState<Read[] | null>(null)

  React.useEffect(() => {
    let current = true

    api("GET /api/analytics")
      .then((found) => current && setReads(found))
      .catch(() => current && setReads([]))

    return () => {
      current = false
    }
  }, [])

  if (reads === null) {
    return <DashboardLoading />
  }

  const views = reads.reduce((all, r) => all + r.views, 0)

  // Group by day for the curve
  const byDayMap = new Map<string, number>()
  for (const r of reads) {
    byDayMap.set(r.on_day, (byDayMap.get(r.on_day) ?? 0) + r.views)
  }
  const points = Array.from(byDayMap.entries()).map(([on_day, count]) => ({
    on_day,
    count,
  }))

  // Group by page for the bars
  const byPageMap = new Map<string, number>()
  for (const r of reads) {
    byPageMap.set(r.path, (byPageMap.get(r.path) ?? 0) + r.views)
  }
  const slices = Array.from(byPageMap.entries())
    .sort((a, b) => b[1] - a[1])
    .slice(0, 10)
    .map(([name, count]) => ({ name, count }))

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`Visitors`}
        description={t`Understand which pages are being read without keeping personal identities.`}
      />

      <div className="grid gap-4 sm:grid-cols-2">
        <Figure label={t`Page views`} value={views} icon={Eye} />
        <Figure label={t`Pages read`} value={byPageMap.size} icon={Users} />
      </div>

      {views === 0 && (
        <p className="text-sm text-muted-foreground">
          {t`Nothing counted yet. Numbers appear as soon as the published site has a visitor.`}
        </p>
      )}

      <Panel title={t`Views by day`}>
        {points.length > 0 ? (
          <Curve points={points} />
        ) : (
          <p className="py-6 text-center text-sm text-muted-foreground">
            {t`The curve draws itself from the first visit on.`}
          </p>
        )}
      </Panel>

      <Panel title={t`Pages`}>
        <Bars slices={slices} empty={t`No pages counted yet.`} />
      </Panel>
    </div>
  )
}
