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

import { api, every } from "@/lib/api"
import { type PluginId, usePlugins } from "@/lib/plugins"
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
  const { activePlugins, ready: pluginsReady } = usePlugins()
  const [stats, setStats] = React.useState<Stats | "failed" | null>(null)

  React.useEffect(() => {
    if (!pluginsReady) return undefined
    let current = true
    // Keep the request lazy. Passing a Promise here would already start the
    // fetch before the plugin check runs, which makes an inactive plugin
    // visible to the backend even though its card is rendered as empty.
    const page = <T,>(
      plugin: PluginId,
      request: () => Promise<T>,
      empty: T,
    ): Promise<T> =>
      activePlugins.has(plugin) ? request() : Promise.resolve(empty)
    Promise.all([
      page("writing", () => every("content.list", { query: {} }), []),
      page("forms", () => every("forms.list", { query: {} }), []),
      page("messaging", () => every("mail.lists.list", { query: {} }), []),
      page("writing", () => every("media.files.list", { query: {} }), []),
      page("learning", () => every("courses.students.list", { query: {} }), []),
      page("commerce", () => every("shop.orders.list", { query: {} }), []),
      page("automation", () => every("automation.flows.list", { query: {} }), []),
      page(
        "automation",
        () => api("workflows.runs.list"),
        { items: [], next_cursor: null },
      ),
    ])
      .then(
        ([content, forms, lists, files, students, orders, flows, runs]) =>
          current &&
          setStats({
            writings: content.length,
            published: content.filter((one) =>
              JSON.stringify(one.publication).includes("published")
            ).length,
            forms: forms.length,
            unread: 0,
            readers: lists.reduce((sum, one) => sum + one.subscriber_count, 0),
            files: files.length,
            bytes: files.reduce((sum, one) => sum + one.bytes, 0),
            students: students.length,
            orders: orders.length,
            flows_on: flows.filter((one) => one.enabled).length,
            work_given_up_on: runs.items.filter((run) => run.status === "failed").length,
          })
      )
      .catch(() => {
        if (current) {
          setStats("failed")
        }
      })
    return () => {
      current = false
    }
  }, [activePlugins, pluginsReady])

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

interface Stats {
  writings: number
  published: number
  forms: number
  unread: number
  readers: number
  files: number
  bytes: number
  students: number
  orders: number
  flows_on: number
  work_given_up_on: number
}
