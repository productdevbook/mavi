/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
  Clock,
  Database,
  HardDrive,
  Hammer,
  ListOrdered,
  Loader2,
  Mail,
} from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { formatBytes } from "@/lib/editor-utils"
import type { Usage } from "../../server/types/mavicms"
import { Figure, Panel } from "@/components/charts"

export const Route = createFileRoute("/dashboard/usage")({
  component: UsageRoute,
})

/**
 * What this installation is holding and what it has done.
 *
 * The answer to "why is my disk full", "did that campaign actually go out"
 * and "is the builder getting slower" — asked at the moment the machine is
 * already unhappy, which is why it is one screen rather than a database
 * somebody has to be found to query. No price anywhere on it: what a site is
 * allowed to hold lives on the settings screen, and this is only what it has
 * taken.
 */
function UsageRoute() {
  const { t } = useLingui()
  const [answer, setAnswer] = React.useState<Usage | "failed" | null>(null)

  React.useEffect(() => {
    let current = true

    api("GET /api/site/usage")
      .then((found) => current && setAnswer(found))
      .catch((why: unknown) => {
        if (!current) return
        toast.error(said(why))
        setAnswer("failed")
      })

    return () => {
      current = false
    }
  }, [])

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

  const waitSeconds = secondsSince(answer.queue.oldest_waiting_since)

  let oldestWaiting: string | undefined

  if (waitSeconds !== null) {
    if (waitSeconds < 60) {
      oldestWaiting = t`oldest: just now`
    } else if (waitSeconds < 3600) {
      oldestWaiting = t`oldest: ${Math.round(waitSeconds / 60)}m`
    } else if (waitSeconds < 86_400) {
      oldestWaiting = t`oldest: ${Math.round(waitSeconds / 3600)}h`
    } else {
      oldestWaiting = t`oldest: ${Math.round(waitSeconds / 86_400)}d`
    }
  }

  const kindNames: Record<string, string> = {
    posts: t`Posts`,
    media: t`Media`,
    products: t`Products`,
    orders: t`Orders`,
    students: t`Students`,
    form_submissions: t`Form submissions`,
    subscribers: t`Subscribers`,
    users: t`Accounts`,
    cards: t`Cards`,
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-lg font-semibold">{t`What this is holding`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`Storage, mail, builds and the queue — no price anywhere on it.`}
        </p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Figure
          label={t`Storage used`}
          value={formatBytes(answer.storage.used_bytes)}
          icon={HardDrive}
        />
        <Figure
          label={t`Mail attempted`}
          value={answer.mail.attempted}
          hint={t`${answer.mail.delivered} delivered, ${answer.mail.bounced} bounced`}
          icon={Mail}
        />
        <Figure
          label={t`Queue waiting`}
          value={answer.queue.waiting}
          hint={oldestWaiting}
          icon={Clock}
          tone={answer.queue.dead > 0 ? "warn" : undefined}
        />
        <Figure
          label={t`Last build`}
          value={
            answer.builds[0]?.seconds != null
              ? t`${answer.builds[0].seconds}s`
              : "—"
          }
          hint={answer.builds[0]?.state}
          icon={Hammer}
        />
      </div>

      <Panel title={t`Storage by kind`}>
        {answer.storage.by_kind.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {t`Nothing uploaded yet.`}
          </p>
        ) : (
          <div className="flex flex-col divide-y divide-border">
            {answer.storage.by_kind.map((kind) => (
              <div key={kind.kind} className="flex items-center gap-3 py-2">
                <span className="min-w-0 flex-1 truncate text-sm capitalize">
                  {kind.kind}
                </span>
                <span className="text-xs text-muted-foreground tabular-nums">
                  {t`${kind.count} files`}
                </span>
                <span className="text-sm tabular-nums">
                  {formatBytes(kind.bytes)}
                </span>
              </div>
            ))}
          </div>
        )}
      </Panel>

      <Panel title={t`Rows by kind`}>
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {answer.rows.map((row) => (
            <div
              key={row.kind}
              className="flex items-center justify-between rounded-lg border p-3"
            >
              <span className="flex items-center gap-2 text-sm text-muted-foreground">
                <ListOrdered className="size-4" />
                {kindNames[row.kind] ?? row.kind}
              </span>
              <span
                className="text-sm font-semibold tabular-nums"
                title={row.exact ? undefined : t`Estimated, on a table this large`}
              >
                {row.exact ? "" : "≈"}
                {row.rows}
              </span>
            </div>
          ))}
        </div>
      </Panel>

      <Panel title={t`Recent builds`}>
        {answer.builds.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {t`Nothing has been published yet.`}
          </p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b text-left text-xs text-muted-foreground">
                  <th className="py-2 pr-3 font-normal">{t`State`}</th>
                  <th className="py-2 pr-3 font-normal">{t`Took`}</th>
                  <th className="py-2 font-normal">{t`Finished`}</th>
                </tr>
              </thead>
              <tbody>
                {answer.builds.map((build, index) => (
                  <tr
                    key={`${build.state}-${build.finished_at ?? index}`}
                    className="border-b last:border-0"
                  >
                    <td className="py-2 pr-3 capitalize">{build.state}</td>
                    <td className="py-2 pr-3 tabular-nums">
                      {build.seconds != null ? t`${build.seconds}s` : "—"}
                    </td>
                    <td className="py-2 tabular-nums">
                      {build.finished_at
                        ? new Date(build.finished_at).toLocaleString()
                        : "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Panel>

      <Panel title={t`Queue`}>
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <Figure label={t`Waiting`} value={answer.queue.waiting} icon={Clock} />
          <Figure label={t`Running`} value={answer.queue.running} />
          <Figure
            label={t`Failed`}
            value={answer.queue.failed}
            tone={answer.queue.failed > 0 ? "warn" : undefined}
          />
          <Figure
            label={t`Dead`}
            value={answer.queue.dead}
            tone={answer.queue.dead > 0 ? "warn" : undefined}
          />
        </div>
      </Panel>

      <Panel title={t`Mail`}>
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <Figure
            label={t`Attempted`}
            value={answer.mail.attempted}
            icon={Database}
          />
          <Figure label={t`Delivered`} value={answer.mail.delivered} />
          <Figure
            label={t`Bounced`}
            value={answer.mail.bounced}
            tone={answer.mail.bounced > 0 ? "warn" : undefined}
          />
          <Figure
            label={t`Failed`}
            value={answer.mail.failed}
            tone={answer.mail.failed > 0 ? "warn" : undefined}
          />
        </div>
      </Panel>
    </div>
  )
}

/** Seconds since a timestamp, or null where there was none to measure from. */
function secondsSince(at: string | null | undefined): number | null {
  if (!at) return null

  return Math.max(0, (Date.now() - new Date(at).getTime()) / 1000)
}
