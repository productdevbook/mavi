import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Clock3, Globe2, Loader2, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { SiteSettings } from "@api"
import { DashboardPageHeader } from "@/components/dashboard/dashboard-page"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

/** Site configuration owned by the canonical settings contract. */
export function SettingsPage() {
  const { t } = useLingui()
  const [site, setSite] = React.useState<SiteSettings | null>(null)
  const [name, setName] = React.useState("")
  const [timezone, setTimezone] = React.useState("")
  const [canonicalUrl, setCanonicalUrl] = React.useState("")
  const [trashRetentionDays, setTrashRetentionDays] = React.useState("30")
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    api("settings.read")
      .then((found) => {
        setSite(found)
        setName(found.name)
        setTimezone(found.timezone)
        setCanonicalUrl(found.canonical_url ?? "")
        setTrashRetentionDays(String(found.trash_retention.days))
      })
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setSite(null)
      })
  }, [])

  React.useEffect(load, [load])

  const save = async () => {
    setBusy(true)

    try {
      const retentionDays = Number(trashRetentionDays)
      const updated = await api("settings.update", {
        body: {
          name: name.trim(),
          timezone: timezone.trim(),
          canonical_url: canonicalUrl.trim() || null,
          trash_retention: { days: retentionDays },
        },
      })
      setSite(updated)
      setName(updated.name)
      setTimezone(updated.timezone)
      setCanonicalUrl(updated.canonical_url ?? "")
      setTrashRetentionDays(String(updated.trash_retention.days))
      toast.success(t`Saved`)
    } catch (why) {
      toast.error(apiMessage(why))
    } finally {
      setBusy(false)
    }
  }

  const retentionDays = Number(trashRetentionDays)
  const validRetentionDays =
    Number.isInteger(retentionDays) && retentionDays >= 1 && retentionDays <= 3650

  return (
    <div className="flex max-w-2xl flex-col gap-8">
      <DashboardPageHeader
        title={t`This site`}
        description={t`The public identity and URL settings used by this site.`}
      />

      <section className="flex flex-col gap-4 rounded-xl border border-border p-4">
        <div className="flex items-center gap-2">
          <Globe2 className="size-4 text-muted-foreground" />
          <h2 className="text-sm font-medium">{t`Public identity`}</h2>
        </div>

        <div className="flex flex-col gap-1.5">
          <Label htmlFor="site-name">{t`Site name`}</Label>
          <Input
            id="site-name"
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
        </div>

        <div className="flex flex-col gap-1.5">
          <Label htmlFor="canonical-url">{t`Canonical URL`}</Label>
          <Input
            id="canonical-url"
            type="url"
            placeholder="https://example.com"
            value={canonicalUrl}
            onChange={(event) => setCanonicalUrl(event.target.value)}
          />
          <p className="text-xs text-muted-foreground">
            {t`Leave empty to clear it. This is the public origin used in generated links.`}
          </p>
        </div>
      </section>

      <section className="flex flex-col gap-4 rounded-xl border border-border p-4">
        <div className="flex items-center gap-2">
          <Trash2 className="size-4 text-muted-foreground" />
          <h2 className="text-sm font-medium">{t`Trash retention`}</h2>
        </div>

        <div className="flex flex-col gap-1.5">
          <Label htmlFor="trash-retention-days">{t`Keep deleted items (days)`}</Label>
          <Input
            id="trash-retention-days"
            type="number"
            min={1}
            max={3650}
            step={1}
            value={trashRetentionDays}
            onChange={(event) => setTrashRetentionDays(event.target.value)}
          />
          <p className="text-xs text-muted-foreground">
            {t`After this period, deleted resources such as content, forms, files and taxonomy terms are permanently removed. Use 1 to 3,650 days.`}
          </p>
        </div>
      </section>

      <section className="flex flex-col gap-4 rounded-xl border border-border p-4">
        <div className="flex items-center gap-2">
          <Clock3 className="size-4 text-muted-foreground" />
          <h2 className="text-sm font-medium">{t`Time`}</h2>
        </div>

        <div className="flex flex-col gap-1.5">
          <Label htmlFor="site-timezone">{t`Timezone`}</Label>
          <Input
            id="site-timezone"
            placeholder="Europe/Berlin"
            value={timezone}
            onChange={(event) => setTimezone(event.target.value)}
          />
          <p className="text-xs text-muted-foreground">
            {t`Use an IANA timezone such as Europe/Berlin or UTC.`}
          </p>
        </div>

        <Button
          className="self-start"
          disabled={
            !site ||
            !name.trim() ||
            !timezone.trim() ||
            !validRetentionDays ||
            busy
          }
          onClick={() => void save()}
        >
          {busy && <Loader2 className="size-4 animate-spin" />}
          {t`Save settings`}
        </Button>
      </section>
    </div>
  )
}
