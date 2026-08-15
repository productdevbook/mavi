/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Eye, Loader2, Users } from "lucide-react"

import { api } from "@/lib/v1"
import type { Summary } from "../../server/types/mavicms"
import { Bars, Curve, Figure, Panel } from "@/components/charts"

export const Route = createFileRoute("/dashboard/visitors")({
  component: VisitorsRoute,
})

const WINDOWS = [7, 28, 90]

/**
 * Who has been reading, counted where the pages are served.
 *
 * No script runs on the site and nothing is kept that says who anybody was:
 * a day's salt turns an address into a mark, the mark says whether this day
 * has seen them before, and the mark goes when the day is over. What is left
 * is how many, and which pages.
 */
function VisitorsRoute() {
  const { t } = useLingui()
  const [days, setDays] = React.useState(28)
  const [answer, setAnswer] = React.useState<Summary | "failed" | null>(null)

  React.useEffect(() => {
    let current = true

    api("GET /api/analytics", { query: { days } })
      .then((found) => current && setAnswer(found))
      .catch(() => current && setAnswer("failed"))

    return () => {
      current = false
    }
  }, [days])

  if (answer === "failed") {
    return (
      <p className="text-sm text-muted-foreground">
        {t`The numbers could not be read just now.`}
      </p>
    )
  }

  if (!answer) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const views = answer.days.reduce((all, day) => all + day.views, 0)
  const visitors = answer.days.reduce((all, day) => all + day.visitors, 0)

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-3">
        <h1 className="text-lg font-semibold">{t`Visitors`}</h1>
        <div className="flex gap-1">
          {WINDOWS.map((window) => (
            <button
              key={window}
              type="button"
              onClick={() => setDays(window)}
              className={`rounded-md px-2.5 py-1 text-xs ${
                days === window
                  ? "bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:bg-muted"
              }`}
            >
              {t`${window} days`}
            </button>
          ))}
        </div>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <Figure label={t`Visitors`} value={visitors} icon={Users} />
        <Figure label={t`Page views`} value={views} icon={Eye} />
      </div>

      {views === 0 && (
        <p className="text-sm text-muted-foreground">
          {t`Nothing counted yet. Numbers appear as soon as the published site has a visitor.`}
        </p>
      )}

      <Panel title={t`Views by day`} aside={t`${days} days`}>
        {answer.days.length > 0 ? (
          <Curve
            points={[...answer.days]
              .reverse()
              .map((day) => ({ on_day: day.on_day, count: day.views }))}
          />
        ) : (
          <p className="py-6 text-center text-sm text-muted-foreground">
            {t`The curve draws itself from the first visit on.`}
          </p>
        )}
      </Panel>

      <Panel title={t`Pages`}>
        <Bars
          slices={answer.pages.map((page) => ({
            name: page.path,
            count: page.views,
          }))}
          empty={t`No pages counted yet.`}
        />
      </Panel>
    </div>
  )
}
