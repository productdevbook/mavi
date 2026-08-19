import { useLingui } from "@lingui/react/macro"
import { ArrowLeft } from "lucide-react"

import { DashboardPageHeader } from "@/components/dashboard/dashboard-page"
import { Button } from "@/components/ui/button"

export function CampaignPage({
  campaignId,
  onBack,
}: {
  campaignId: string
  onBack: () => void
}) {
  const { t } = useLingui()

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <DashboardPageHeader
        title={t`Campaign #${campaignId}`}
        description={t`Mail sendings are processed in the background directly from your lists.`}
        actions={
          <Button variant="ghost" size="sm" onClick={onBack}>
            <ArrowLeft /> {t`Mail`}
          </Button>
        }
      />
    </div>
  )
}
