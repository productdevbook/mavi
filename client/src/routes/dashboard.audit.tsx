/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Download, Loader2 } from "lucide-react"
import { toast } from "sonner"

import { record, RECORD_AS_A_FILE, type Entry } from "@/lib/v1-audit"
import { said } from "@/lib/v1-said"
import { AuditTable } from "@/components/audit-record"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"

export const Route = createFileRoute("/dashboard/audit")({
  component: AuditRoute,
})

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
function AuditRoute() {
  const { t } = useLingui()

  const [entries, setEntries] = React.useState<Entry[] | null>(null)
  const [action, setAction] = React.useState("")
  const [subject, setSubject] = React.useState("")
  const [next, setNext] = React.useState<string | null>(null)
  const [more, setMore] = React.useState(false)

  const load = React.useCallback(
    (after?: string) => {
      record({
        did: action || undefined,
        about: subject || undefined,
        after,
        limit: PAGE,
      })
        .then((page) => {
          setNext(page.next ?? null)
          setEntries((held) =>
            after ? [...(held ?? []), ...page.items] : page.items
          )
        })
        .catch((why: unknown) => {
          toast.error(said(why))
          setEntries((held) => held ?? [])
        })
        .finally(() => setMore(false))
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
    <>
      <div className="mb-6 flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">{t`Record`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`Who did what to this site, and when.`}
          </p>
        </div>

        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            window.location.href = RECORD_AS_A_FILE
          }}
        >
          <Download /> {t`Download`}
        </Button>
      </div>

      <div className="mb-4 flex flex-wrap gap-2">
        <Input
          value={action}
          onChange={(event) => setAction(event.target.value)}
          placeholder={t`Anything done`}
          className="h-9 w-56"
        />

        <Input
          value={subject}
          onChange={(event) => setSubject(event.target.value)}
          placeholder={t`Anything it was done to`}
          className="h-9 w-56"
        />
      </div>

      {entries === null ? (
        <Loader2 className="size-4 animate-spin text-muted-foreground" />
      ) : entries.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          {t`Nothing here yet.`}
        </p>
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
    </>
  )
}
