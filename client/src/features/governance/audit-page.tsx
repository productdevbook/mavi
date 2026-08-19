import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Download, Loader2 } from "lucide-react"
import { toast } from "sonner"

import { record, RECORD_AS_A_FILE, type Entry } from "@/lib/v1-audit"
import { said } from "@/lib/v1-said"
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

  const [entries, setEntries] = React.useState<Entry[] | null>(null)
  const [action, setAction] = React.useState("")
  const [subject, setSubject] = React.useState("")
  const [next, setNext] = React.useState<string | null>(null)
  const [more, setMore] = React.useState(false)
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

      record({
        did: action || undefined,
        about: subject || undefined,
        after,
        limit: PAGE,
      })
        .then((page) => {
          if (current !== request.current) return
          setNext(page.next ?? null)
          setEntries((held) =>
            after ? [...(held ?? []), ...page.items] : page.items
          )
        })
        .catch((why: unknown) => {
          if (current !== request.current) return
          toast.error(said(why))
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
            onClick={() => {
              window.location.href = RECORD_AS_A_FILE
            }}
          >
            <Download /> {t`Download`}
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
