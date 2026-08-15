/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { ArrowLeft, Loader2, Send } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Campaign } from "../../server/types/mavicms"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Figure } from "@/components/charts"

export const Route = createFileRoute("/dashboard/mail_/campaigns/$campaignId")({
  component: CampaignRoute,
})

/** How often to look again while one is going out. */
const WATCH_INTERVAL = 5000

/**
 * One campaign, and how far it has got.
 *
 * Sending is a queue rather than a request: what this screen shows is how many
 * have gone, and pressing send twice does not send it twice — the campaign's
 * own state is what decides that, not this button being disabled.
 */
function CampaignRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const { campaignId } = Route.useParams()

  const [campaign, setCampaign] = React.useState<Campaign | null>(null)
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    every("GET /api/mail/campaigns")
      .then((all) => setCampaign(all.find((one) => one.id === campaignId) ?? null))
      .catch((why: unknown) => {
        toast.error(said(why))
        setCampaign(null)
      })
  }, [campaignId])

  React.useEffect(load, [load])

  React.useEffect(() => {
    if (campaign?.state !== "sending") return

    const timer = window.setInterval(load, WATCH_INTERVAL)

    return () => window.clearInterval(timer)
  }, [campaign?.state, load])

  const send = async () => {
    setBusy(true)

    try {
      await api("POST /api/mail/campaigns/{id}/send", {
        path: { id: campaignId },
      })
      toast.success(t`On its way. Sending happens in the background.`)
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  if (!campaign) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const states: Record<string, string> = {
    draft: t`Not sent`,
    sending: t`Sending`,
    sent: t`Sent`,
  }

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
        <h1 className="text-lg font-semibold">{campaign.subject}</h1>
        <Badge variant={campaign.state === "sent" ? "default" : "secondary"}>
          {states[campaign.state] ?? campaign.state}
        </Badge>

        {campaign.state === "draft" && (
          <Button className="ml-auto" disabled={busy} onClick={() => void send()}>
            {busy ? <Loader2 className="animate-spin" /> : <Send />}
            {t`Send it`}
          </Button>
        )}
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <Figure label={t`Sent`} value={campaign.sent_count} icon={Send} />
        <Figure
          label={t`Written`}
          value={new Date(campaign.created_at).toLocaleDateString()}
          icon={Send}
        />
      </div>
    </div>
  )
}
