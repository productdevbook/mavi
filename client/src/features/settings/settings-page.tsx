import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Clock3, Globe2, Loader2 } from "lucide-react"
import { toast } from "sonner"

import { nextApi } from "@/lib/server-next"
import { serverNextMessage } from "@/lib/server-next-auth"
import type { SiteSettings } from "@api-next"
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
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    nextApi("settings.read")
      .then((found) => {
        setSite(found)
        setName(found.name)
        setTimezone(found.timezone)
        setCanonicalUrl(found.canonical_url ?? "")
      })
      .catch((why: unknown) => {
        toast.error(serverNextMessage(why))
        setSite(null)
      })
  }, [])

  React.useEffect(load, [load])

  const save = async () => {
    setBusy(true)

    try {
      const updated = await nextApi("settings.update", {
        body: {
          name: name.trim(),
          timezone: timezone.trim(),
          canonical_url: canonicalUrl.trim() || null,
        },
      })
      setSite(updated)
      setName(updated.name)
      setTimezone(updated.timezone)
      setCanonicalUrl(updated.canonical_url ?? "")
      toast.success(t`Saved`)
    } catch (why) {
      toast.error(serverNextMessage(why))
    } finally {
      setBusy(false)
    }
  }

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
          disabled={!site || !name.trim() || !timezone.trim() || busy}
          onClick={() => void save()}
        >
          {busy && <Loader2 className="size-4 animate-spin" />}
          {t`Save settings`}
        </Button>
      </section>
    </div>
  )
}
