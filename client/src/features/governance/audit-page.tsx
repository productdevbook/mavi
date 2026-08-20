import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Download, Loader2 } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { AuditEvent } from "@api"
import { AuditTable } from "@/components/audit-record"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  DashboardEmpty,
  DashboardError,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

/** How many arrive at once, and how many more each time the button is pressed. */
const PAGE = 50

/**
 * Who did what, and when.
 *
 * Read on a bad day and never before one, which is what the page is built
 * around: the things that cannot be undone are marked so they are found by
 * scanning rather than by reading, and the filter is two boxes rather than a
 * query language.
 */
export function AuditPage() {
  const { t } = useLingui()

  const [entries, setEntries] = React.useState<AuditEvent[] | null>(null)
  const [action, setAction] = React.useState("")
  const [subject, setSubject] = React.useState("")
  const [next, setNext] = React.useState<string | null>(null)
  const [more, setMore] = React.useState(false)
  const [exporting, setExporting] = React.useState(false)
  const [error, setError] = React.useState(false)
  const request = React.useRef(0)

  const load = React.useCallback(
    (after?: string) => {
      const current = ++request.current

      if (!after) {
        setEntries(null)
        setError(false)
        setNext(null)
      }

      api("audit.events.list", {
        query: {
          action: action || undefined,
          resource_type: subject || undefined,
          after,
          limit: PAGE,
        },
      })
        .then((page) => {
          if (current !== request.current) return
          setNext(page.next_cursor ?? null)
          setEntries((held) =>
            after ? [...(held ?? []), ...page.items] : page.items
          )
        })
        .catch((why: unknown) => {
          if (current !== request.current) return
          toast.error(apiMessage(why))
          if (after) return
          setError(true)
          setEntries([])
        })
        .finally(() => {
          if (current === request.current) setMore(false)
        })
    },
    [action, subject]
  )

  // Both boxes are typed into, so the fetch waits for a pause rather than
  // firing per keystroke.
  React.useEffect(() => {
    const timer = setTimeout(() => load(), 250)
    return () => clearTimeout(timer)
  }, [load])

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`Record`}
        description={t`Who did what to this site, and when.`}
        actions={
          <Button
            variant="outline"
            size="sm"
            disabled={exporting}
            onClick={() => {
              setExporting(true)
              api("audit.events.export", {
                query: {
                  action: action || undefined,
                  resource_type: subject || undefined,
                  limit: 10000,
                },
              })
                .then((exported) => {
                  const file = new Blob([JSON.stringify(exported, null, 2)], {
                    type: "application/json",
                  })
                  const url = URL.createObjectURL(file)
                  const link = document.createElement("a")
                  link.href = url
                  link.download = `audit-${new Date().toISOString().slice(0, 10)}.json`
                  link.click()
                  URL.revokeObjectURL(url)
                  if (exported.truncated) {
                    toast.warning(t`The export reached its 10,000-event limit.`)
                  }
                })
                .catch((why: unknown) => toast.error(apiMessage(why)))
                .finally(() => setExporting(false))
            }}
          >
            {exporting ? <Loader2 className="animate-spin" /> : <Download />} {t`Download`}
          </Button>
        }
      />

      <div className="grid max-w-2xl gap-3 sm:grid-cols-2">
        <Input
          aria-label={t`Filter by action`}
          value={action}
          onChange={(event) => setAction(event.target.value)}
          placeholder={t`Anything done`}
        />

        <Input
          aria-label={t`Filter by subject`}
          value={subject}
          onChange={(event) => setSubject(event.target.value)}
          placeholder={t`Anything it was done to`}
        />
      </div>

      {entries === null ? (
        <DashboardLoading />
      ) : error ? (
        <DashboardError message={t`The record could not be read just now.`} />
      ) : entries.length === 0 ? (
        <DashboardEmpty
          title={t`Nothing here yet.`}
          description={t`Changes made to this site will appear here.`}
        />
      ) : (
        <>
          <AuditTable entries={entries} />

          {next === null ? null : (
            <Button
              variant="outline"
              size="sm"
              className="mt-3"
              disabled={more}
              onClick={() => {
                setMore(true)
                load(next)
              }}
            >
              {more ? <Loader2 className="animate-spin" /> : null}
              {t`Older`}
            </Button>
          )}
        </>
      )}

      <p className="mt-6 max-w-2xl text-xs text-muted-foreground">
        {t`Passwords, keys and tokens are never written here — an entry says a key was replaced, not what it was replaced with.`}
      </p>
    </div>
  )
}
