/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute, useNavigate } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const CampaignPage = lazyRoute(() =>
  import("@/features/mail/campaign-page").then(({ CampaignPage }) => ({
    default: CampaignPage,
  })),
)

export const Route = createFileRoute("/dashboard/mail_/campaigns/$campaignId")({
  component: CampaignRoute,
})

function CampaignRoute() {
  const navigate = useNavigate()
  const { campaignId } = Route.useParams()

  return (
    <CampaignPage
      campaignId={campaignId}
      onBack={() => void navigate({ to: "/dashboard/mail" })}
    />
  )
}
