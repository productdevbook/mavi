import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Clock, HardDrive, ListOrdered, Mail } from "lucide-react"
import { toast } from "sonner"

import { every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import { formatBytes } from "@/lib/editor-utils"
import { Figure } from "@/components/charts"
import {
  DashboardError,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

/**
 * What this installation is holding and what it has done.
 */
export function UsagePage() {
  const { t } = useLingui()
  const [answer, setAnswer] = React.useState<Usage | "failed" | null>(null)

  React.useEffect(() => {
    let current = true

    Promise.all([
      every("media.files.list", { query: {} }),
      every("content.list", { query: {} }),
      every("mail.lists.list", { query: {} }),
      every("jobs.list", { query: {} }),
    ])
      .then(([files, content, lists, jobs]) =>
        current &&
        setAnswer({
          bytes: files.reduce((sum, file) => sum + file.bytes, 0),
          files: files.length,
          writings: content.length,
          published: content.filter((one) =>
            JSON.stringify(one.publication).includes("published")
          ).length,
          readers: lists.reduce((sum, list) => sum + list.subscriber_count, 0),
          work_given_up_on: jobs.filter((job) => job.state === "dead").length,
        })
      )
      .catch((why: unknown) => {
        if (!current) return
        toast.error(apiMessage(why))
        setAnswer("failed")
      })

    return () => {
      current = false
    }
  }, [])

  if (answer === "failed") {
    return (
      <DashboardError message={t`The numbers could not be read just now.`} />
    )
  }

  if (!answer) {
    return <DashboardLoading />
  }

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`What this is holding`}
        description={t`Storage, writings, readers and the queue — no price anywhere on it.`}
      />

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Figure
          label={t`Storage used`}
          value={formatBytes(answer.bytes)}
          hint={t`${answer.files} files`}
          icon={HardDrive}
        />
        <Figure
          label={t`Writings`}
          value={answer.writings}
          hint={t`${answer.published} published`}
          icon={ListOrdered}
        />
        <Figure label={t`Readers`} value={answer.readers} icon={Mail} />
        <Figure
          label={t`Queue issues`}
          value={answer.work_given_up_on}
          hint={answer.work_given_up_on > 0 ? t`issues found` : t`clean`}
          icon={Clock}
          tone={answer.work_given_up_on > 0 ? "warn" : undefined}
        />
      </div>
    </div>
  )
}

interface Usage {
  bytes: number
  files: number
  writings: number
  published: number
  readers: number
  work_given_up_on: number
}
