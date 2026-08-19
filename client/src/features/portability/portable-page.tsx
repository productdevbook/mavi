import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Download, FileJson, Loader2, Upload } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { ImportReceipt } from "@api"
import { DashboardPageHeader } from "@/components/dashboard/dashboard-page"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"

export function PortablePage() {
  const { t } = useLingui()
  const chooser = React.useRef<HTMLInputElement>(null)
  const [exporting, setExporting] = React.useState(false)
  const [importing, setImporting] = React.useState(false)
  const [summary, setSummary] = React.useState<ImportReceipt | null>(null)

  const take = async () => {
    setExporting(true)

    try {
      const bundle = await api("portable.export")
      const url = URL.createObjectURL(
        new Blob([JSON.stringify(bundle, null, 2)], {
          type: "application/json",
        })
      )
      const link = document.createElement("a")

      link.href = url
      link.download = `site-${new Date().toISOString().slice(0, 10)}.json`
      link.click()
      setTimeout(() => URL.revokeObjectURL(url), 0)
    } catch (why) {
      toast.error(apiMessage(why))
    } finally {
      setExporting(false)
    }
  }

  const read = async (files: FileList | null) => {
    const file = files?.[0]

    if (!file) return

    setImporting(true)
    setSummary(null)

    try {
      const bundle = JSON.parse(await file.text())
      const result = await api("portable.import", {
        body: { bundle, strategy: "create_only" },
      })

      setSummary(result)
      toast.success(t`The copy was read in.`)
    } catch (why) {
      toast.error(
        why instanceof SyntaxError
          ? t`That is not a copy of a site.`
          : apiMessage(why)
      )
    } finally {
      setImporting(false)
      if (chooser.current) chooser.current.value = ""
    }
  }

  const busy = exporting || importing

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <DashboardPageHeader
        title={t`Import and export`}
        description={t`Move the supported site content as a versioned JSON bundle. Accounts, uploads, orders, and secrets stay out of the file.`}
      />

      <div className="grid gap-4 md:grid-cols-2">
        <section className="flex flex-col gap-3 rounded-xl border border-border p-4">
          <div className="flex items-center gap-2">
            <Download className="size-4 text-muted-foreground" />
            <h2 className="text-sm font-medium">{t`Export`}</h2>
          </div>
          <p className="flex-1 text-sm text-muted-foreground">
            {t`Download languages, content types, taxonomy, and writings with their revisions and assignments.`}
          </p>
          <Button
            className="self-start"
            variant="outline"
            disabled={busy}
            onClick={() => void take()}
          >
            {exporting ? <Loader2 className="animate-spin" /> : <Download />}
            {t`Download a copy`}
          </Button>
        </section>

        <section className="flex flex-col gap-3 rounded-xl border border-border p-4">
          <div className="flex items-center gap-2">
            <Upload className="size-4 text-muted-foreground" />
            <h2 className="text-sm font-medium">{t`Import`}</h2>
          </div>
          <p className="flex-1 text-sm text-muted-foreground">
            {t`Read a bundle from another site. Existing content is left alone; only valid, new records are added.`}
          </p>
          <Button
            className="self-start"
            variant="outline"
            disabled={busy}
            onClick={() => chooser.current?.click()}
          >
            {importing ? <Loader2 className="animate-spin" /> : <Upload />}
            {t`Choose a JSON copy`}
          </Button>
          <input
            ref={chooser}
            type="file"
            accept="application/json,.json"
            className="hidden"
            onChange={(event) => void read(event.target.files)}
          />
        </section>
      </div>

      {summary ? (
        <section className="flex flex-col gap-3 rounded-xl border border-border p-4">
          <div className="flex items-center gap-2">
            <FileJson className="size-4 text-muted-foreground" />
            <h2 className="text-sm font-medium">{t`Import complete`}</h2>
            <Badge variant="secondary">{summary.strategy}</Badge>
          </div>
          <div className="grid gap-3 text-sm sm:grid-cols-4">
            <Count label={t`Languages`} value={summary.languages} />
            <Count label={t`Categories and tags`} value={summary.terms} />
            <Count label={t`Writings`} value={summary.content} />
            <Count label={t`Revisions`} value={summary.revisions} />
          </div>
        </section>
      ) : null}
    </div>
  )
}

function Count({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-lg bg-muted/40 px-3 py-2">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="mt-1 text-lg font-semibold tabular-nums">{value}</p>
    </div>
  )
}
