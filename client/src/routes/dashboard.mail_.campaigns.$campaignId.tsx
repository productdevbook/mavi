/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { ArrowLeft } from "lucide-react"

import { Button } from "@/components/ui/button"

export const Route = createFileRoute("/dashboard/mail_/campaigns/$campaignId")({
  component: CampaignRoute,
})

function CampaignRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const { campaignId } = Route.useParams()

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <Button
        variant="ghost"
        size="sm"
        className="-ml-2 self-start"
        onClick={() => void navigate({ to: "/dashboard/mail" })}
      >
        <ArrowLeft /> {t`Mail`}
      </Button>

      <div className="flex flex-wrap items-center gap-3">
        <h1 className="text-lg font-semibold">{t`Campaign`} #{campaignId}</h1>
      </div>

      <p className="text-sm text-muted-foreground">
        {t`Mail sendings are processed in the background directly from your lists.`}
      </p>
    </div>
  )
}
