/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Activity, Gauge, Loader2 } from "lucide-react"

import { api } from "@/lib/v1"
import type { Vitals } from "../../server/types/mavicms"
import { Figure, Panel } from "@/components/charts"

export const Route = createFileRoute("/dashboard/performance")({
  component: PerformanceRoute,
})

const WINDOWS = [7, 28, 90]

/**
 * How fast the site feels to the people actually reading it.
 *
 * Every published page reports what the browser measured as visitors leave;
 * this screen is those reports at the seventy-fifth — what three visitors in
 * four got or better. A lab tool measures one page once from one machine,
 * which is why a page can be poor here and fine there: the benchmark never
 * opened it on a phone on a train.
 */
function PerformanceRoute() {
  const { t } = useLingui()
  const [days, setDays] = React.useState(28)
  const [answer, setAnswer] = React.useState<Vitals | "failed" | null>(null)

  React.useEffect(() => {
    let current = true

    api("GET /api/analytics/vitals", { query: { days } })
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

  const names: Record<string, string> = {
    lcp: t`Largest paint`,
    cls: t`Layout shift`,
    inp: t`Responds to input`,
    ttfb: t`First byte`,
  }

  const measured = answer.scores.reduce((all, score) => all + score.samples, 0)

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-3">
        <h1 className="text-lg font-semibold">{t`Performance`}</h1>
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

      {answer.scores.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          {t`Nothing measured yet. Numbers appear once real visitors reach the published site.`}
        </p>
      ) : (
        <>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            <Figure
              label={t`Measurements`}
              value={measured}
              hint={t`over ${answer.days} days`}
              icon={Gauge}
            />
            <Figure
              label={t`Pages measured`}
              value={answer.worst.length}
              icon={Activity}
            />
          </div>

          <Panel title={t`By measurement`}>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
              {answer.scores.map((score) => (
                <div key={score.kind} className="rounded-lg border p-3">
                  <p className="text-xs text-muted-foreground">
                    {names[score.kind] ?? score.kind.toUpperCase()}
                  </p>
                  <p
                    className={`mt-1 text-2xl font-semibold tabular-nums ${toned(score.verdict)}`}
                  >
                    {reading(score.kind, score.p75)}
                  </p>
                  <p className="mt-1 text-xs text-muted-foreground tabular-nums">
                    {t`${score.samples} measured`}
                  </p>
                </div>
              ))}
            </div>
          </Panel>

          <Panel title={t`Pages, worst first`} aside={t`${answer.days} days`}>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b text-left text-xs text-muted-foreground">
                    <th className="py-2 pr-3 font-normal">{t`Page`}</th>
                    <th className="py-2 pr-3 font-normal">LCP</th>
                    <th className="py-2 pr-3 font-normal">CLS</th>
                    <th className="py-2 pr-3 font-normal">INP</th>
                    <th className="py-2 pr-3 font-normal">TTFB</th>
                    <th className="py-2 font-normal">{t`Measured`}</th>
                  </tr>
                </thead>
                <tbody>
                  {answer.worst.map((page) => (
                    <tr key={page.path} className="border-b last:border-0">
                      <td className="max-w-[22rem] truncate py-2 pr-3 font-mono text-xs">
                        {page.path}
                      </td>
                      <td className="py-2 pr-3 tabular-nums">
                        {reading("lcp", page.lcp)}
                      </td>
                      <td className="py-2 pr-3 tabular-nums">
                        {reading("cls", page.cls)}
                      </td>
                      <td className="py-2 pr-3 tabular-nums">
                        {reading("inp", page.inp)}
                      </td>
                      <td className="py-2 pr-3 tabular-nums">
                        {reading("ttfb", page.ttfb)}
                      </td>
                      <td className="py-2 tabular-nums">{page.samples}</td>
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

function toned(verdict: string): string {
  switch (verdict) {
    case "good":
      return "text-emerald-600 dark:text-emerald-400"
    case "could_be_better":
      return "text-amber-600 dark:text-amber-400"
    case "poor":
      return "text-red-600 dark:text-red-400"
    default:
      return ""
  }
}
