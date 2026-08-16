/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
  AlertTriangle,
  FileText,
  GraduationCap,
  HardDrive,
  Inbox,
  Loader2,
  Mails,
  ShoppingCart,
  Users,
  Workflow,
} from "lucide-react"

import { api } from "@/lib/v1"
import type { Overview } from "@api"
import { Figure } from "@/components/charts"
import { inBytes } from "@/lib/bytes"
import { AddressHealth } from "@/components/dashboard/address-health"

export const Route = createFileRoute("/dashboard/")({
  component: HomeRoute,
})

function HomeRoute() {
  return <SiteHome />
}

/**
 * What a site adds up to.
 */
function SiteHome() {
  const { t } = useLingui()
  const [stats, setStats] = React.useState<Overview | "failed" | null>(null)

  React.useEffect(() => {
    let current = true
    api("GET /api/overview")
      .then((found) => current && setStats(found))
      .catch(() => current && setStats("failed"))
    return () => {
      current = false
    }
  }, [])

  if (stats === "failed") {
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

  return (
    <div className="flex flex-col gap-6">
      <div>
        <h1 className="text-lg font-semibold">{t`Overview`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`Everything this site adds up to.`}
        </p>
      </div>

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Figure
          label={t`Writings`}
          value={stats.writings}
          hint={t`${stats.published} published`}
          icon={FileText}
        />
        <Figure
          label={t`Forms`}
          value={stats.forms}
          hint={
            stats.unread > 0 ? t`${stats.unread} unread` : t`all read`
          }
          icon={Inbox}
          tone={stats.unread > 0 ? "warn" : undefined}
        />
        <Figure
          label={t`Mailing list`}
          value={stats.readers}
          hint={t`readers reached`}
          icon={Mails}
        />
        <Figure
          label={t`Uploads`}
          value={stats.files}
          hint={inBytes(stats.bytes)}
          icon={HardDrive}
        />
      </div>

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Figure
          label={t`Students`}
          value={stats.students}
          hint={t`learning here`}
          icon={stats.students > 0 ? GraduationCap : Users}
        />
        <Figure
          label={t`Orders`}
          value={stats.orders}
          hint={t`orders placed`}
          icon={ShoppingCart}
        />
        <Figure
          label={t`Flows`}
          value={stats.flows_on}
          hint={t`switched on`}
          icon={Workflow}
        />
        <Figure
          label={t`Work backlog`}
          value={stats.work_given_up_on}
          hint={stats.work_given_up_on > 0 ? t`issues found` : t`clean`}
          icon={AlertTriangle}
          tone={stats.work_given_up_on > 0 ? "warn" : undefined}
        />
      </div>

      <AddressHealth />
    </div>
  )
}
