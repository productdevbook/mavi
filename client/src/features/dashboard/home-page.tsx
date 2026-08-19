import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import {
  AlertTriangle,
  FileText,
  GraduationCap,
  HardDrive,
  Inbox,
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
import {
  DashboardError,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

/**
 * What a site adds up to.
 */
export function HomePage() {
  const { t } = useLingui()
  const [stats, setStats] = React.useState<Overview | "failed" | null>(null)

  React.useEffect(() => {
    let current = true
    api("site.overview")
      .then((found) => current && setStats(found))
      .catch(() => current && setStats("failed"))
    return () => {
      current = false
    }
  }, [])

  if (stats === "failed") {
    return (
      <DashboardError message={t`The numbers could not be read just now.`} />
    )
  }

  if (!stats) {
    return <DashboardLoading />
  }

  return (
    <div className="flex flex-col gap-6">
      <DashboardPageHeader
        title={t`Overview`}
        description={t`Everything this site adds up to.`}
      />

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
          hint={stats.unread > 0 ? t`${stats.unread} unread` : t`all read`}
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
