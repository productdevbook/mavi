/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
  Clock,
  HardDrive,
  ListOrdered,
  Loader2,
  Mail,
} from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { formatBytes } from "@/lib/editor-utils"
import type { Overview } from "@api"
import { Figure } from "@/components/charts"

export const Route = createFileRoute("/dashboard/usage")({
  component: UsageRoute,
})

/**
 * What this installation is holding and what it has done.
 */
function UsageRoute() {
  const { t } = useLingui()
  const [answer, setAnswer] = React.useState<Overview | "failed" | null>(null)

  React.useEffect(() => {
    let current = true

    api("GET /api/overview")
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

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-lg font-semibold">{t`What this is holding`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`Storage, writings, readers and the queue — no price anywhere on it.`}
        </p>
      </div>

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
        <Figure
          label={t`Readers`}
          value={answer.readers}
          icon={Mail}
        />
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
