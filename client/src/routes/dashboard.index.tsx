/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { Link, createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
  FileText,
  GraduationCap,
  HardDrive,
  Inbox,
  Loader2,
  Mails,
  Users,
  Workflow,
} from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Issue, Overview } from "@api"
import { Curve, Figure, Panel } from "@/components/charts"
import { inBytes } from "@/lib/bytes"
import { AddressHealth } from "@/components/dashboard/address-health"
import { Discoverability } from "@/components/dashboard/discoverability"

export const Route = createFileRoute("/dashboard/")({
  component: HomeRoute,
})

/** How far back the curves go, and what the buttons offer. */
const WINDOWS = [7, 30, 90]

function HomeRoute() {
  return <SiteHome />
}

/**
 * What a site adds up to.
 *
 * This was the list of posts, which answers "what did I write last" and
 * nothing else — whether anybody is filling in the form, whether the list is
 * growing, whether a flow has been failing quietly for a fortnight were each a
 * page away or nowhere. The list is still a click away, and is where the
 * sidebar's Posts now goes.
 */
function SiteHome() {
  const { t } = useLingui()
  const [days, setDays] = React.useState(30)
  // One value rather than a pair. Two useStates for "loading" and "failed"
  // have a fourth state nobody means — failed and loading at once — and
  // clearing the flag on the way into the effect is a synchronous set that
  // React rightly complains about.
  const [answer, setAnswer] = React.useState<Overview | "failed" | null>(null)

  React.useEffect(() => {
    let current = true
    api("GET /api/overview", { query: { days } })
      .then((found) => current && setAnswer(found))
      .catch(() => current && setAnswer("failed"))
    return () => {
      current = false
    }
  }, [days])

  const stats = answer === "failed" ? null : answer

  if (answer === "failed") {
    return (
      <p className="text-sm text-muted-foreground">
        {t`The numbers could not be read just now.`}
      </p>
    )
  }

  if (!stats) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const { totals, mail } = stats
  const delivered = mail.sent - mail.bounced

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">{t`Overview`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`Everything this site adds up to, and how the last ${days} days went.`}
          </p>
        </div>

        <div className="flex gap-1 rounded-lg border border-border p-0.5">
          {WINDOWS.map((window) => (
            <button
              key={window}
              type="button"
              onClick={() => setDays(window)}
              className={
                window === days
                  ? "rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground"
                  : "rounded-md px-2.5 py-1 text-xs text-muted-foreground hover:text-foreground"
              }
            >
              {t`${window} days`}
            </button>
          ))}
        </div>
      </div>

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Figure
          label={t`Written`}
          value={totals.posts}
          hint={t`${sum(stats.written)} in ${days} days`}
          icon={FileText}
        />
        <Figure
          label={t`Sent through forms`}
          value={totals.submissions}
          hint={
            totals.unseen > 0 ? t`${totals.unseen} not read yet` : t`all read`
          }
          icon={Inbox}
          tone={totals.unseen > 0 ? "warn" : undefined}
        />
        <Figure
          label={t`On the mailing list`}
          value={totals.subscribers}
          hint={t`on the list now`}
          icon={Mails}
        />
        <Figure
          label={t`Uploads`}
          value={totals.media}
          hint={inBytes(totals.media_bytes)}
          icon={HardDrive}
        />
      </div>

      <div className="grid gap-4 lg:grid-cols-3">
        <Panel
          title={t`Written`}
          aside={t`${sum(stats.written)} in ${days} days`}
        >
          <Curve points={stats.written} />
        </Panel>
        <Panel
          title={t`Form submissions`}
          aside={t`${sum(stats.arrived)} in ${days} days`}
        >
          <Curve points={stats.arrived} />
        </Panel>
        <Panel
          title={t`Visitors`}
          aside={t`${stats.visitors.reduce((all, day) => all + day.visitors, 0)} in ${days} days`}
        >
          <Curve
            points={stats.visitors.map((day) => ({
              on_day: day.on_day,
              count: day.visitors,
            }))}
          />
        </Panel>
      </div>

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Figure
          label={t`Mail delivered`}
          value={delivered}
          hint={
            mail.sent > 0
              ? t`of ${mail.sent + mail.failed} tried`
              : t`nothing sent in ${days} days`
          }
          icon={Mails}
        />
        <Figure
          label={t`Bounced or complained`}
          value={mail.bounced + mail.complained}
          // The number that ends an account rather than the one that reads
          // badly: Amazon stops a sender long before a person notices.
          hint={t`of ${mail.sent} sent`}
          icon={Mails}
          tone={
            mail.sent > 0 && (mail.bounced + mail.complained) * 20 > mail.sent
              ? "warn"
              : undefined
          }
        />
        <Figure
          label={t`Flows`}
          value={`${totals.flows_on}/${totals.flows}`}
          hint={t`switched on`}
          icon={Workflow}
        />
        <Figure
          label={t`Students`}
          value={totals.students}
          hint={t`${totals.orders} orders`}
          icon={totals.students > 0 ? GraduationCap : Users}
        />
      </div>

      <WhatIsWrong />

      <AddressHealth />
      <Discoverability />
    </div>
  )
}

function sum(points: { count: number }[]): number {
  return points.reduce((total, one) => total + one.count, 0)
}



/**
 * What is wrong with the pages, before anybody notices.
 *
 * Written when a post is written rather than asked for now, so this is a read
 * of what is already known: a missing description, a title nothing would
 * click, two things answering on one address.
 */
function WhatIsWrong() {
  const { t } = useLingui()
  const [issues, setIssues] = React.useState<Issue[] | null>(null)

  React.useEffect(() => {
    every("GET /api/pages/issues")
      .then(setIssues)
      .catch((why: unknown) => {
        toast.error(said(why))
        setIssues([])
      })
  }, [])

  if (issues === null || issues.length === 0) {
    return null
  }

  return (
    <Panel title={t`Worth looking at`} aside={t`${issues.length} found`}>
      <div className="flex flex-col divide-y divide-border">
        {issues.slice(0, 10).map((issue) => (
          <Link
            key={`${issue.post_id}-${issue.kind}`}
            to="/editor/$postId"
            params={{ postId: issue.post_id }}
            className="flex items-center gap-3 py-2 hover:underline"
          >
            <span className="min-w-0 flex-1 truncate text-sm">
              {issue.title}
            </span>
            <span className="text-xs text-muted-foreground">{issue.kind}</span>
          </Link>
        ))}
      </div>
    </Panel>
  )
}
