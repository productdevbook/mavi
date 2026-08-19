import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import {
  FileText,
  Image as ImageIcon,
  Inbox,
  RotateCcw,
  Tags,
  Trash2,
} from "lucide-react"
import { toast } from "sonner"

import { said } from "@/lib/v1-said"
import { forGood, inTheBin, putBack, type Thrown } from "@/lib/v1-trash"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  DashboardEmpty,
  DashboardError,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from "@/components/ui/item"

export function TrashPage() {
  const { t } = useLingui()

  const [entries, setEntries] = React.useState<Thrown[] | null>(null)
  const [busy, setBusy] = React.useState<string | null>(null)
  const [error, setError] = React.useState(false)

  const load = React.useCallback(() => {
    setError(false)
    inTheBin()
      .then(setEntries)
      .catch((why: unknown) => {
        toast.error(said(why))
        setError(true)
        setEntries([])
      })
  }, [])

  React.useEffect(load, [load])

  // v1 names the table a thing came out of, which is what putting it back
  // needs to know.
  const named: Record<string, string> = {
    writings: t`post`,
    files: t`file`,
    forms: t`form`,
    terms: t`category or tag`,
    products: t`product`,
    courses: t`course`,
    boards: t`board`,
    cards: t`card`,
    flows: t`flow`,
  }

  const icon = (kind: string) => {
    const glyph =
      kind === "files"
        ? ImageIcon
        : kind === "forms"
          ? Inbox
          : kind === "terms"
            ? Tags
            : FileText
    return React.createElement(glyph, {
      className: "size-4 text-muted-foreground",
    })
  }

  const restore = (entry: Thrown) => {
    setBusy(entry.id)
    putBack(entry.kind, entry.id)
      .then(() => {
        toast.success(t`${entry.called} is back`)
        load()
      })
      .catch((why) => toast.error(said(why)))
      .finally(() => setBusy(null))
  }

  const purge = (entry: Thrown) => {
    setBusy(entry.id)
    forGood(entry.kind, entry.id)
      .then(load)
      .catch((why) => toast.error(said(why)))
      .finally(() => setBusy(null))
  }

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`Bin`}
        description={t`Deleted content waits thirty days before it is removed permanently, and can be restored in the meantime.`}
      />

      <Card>
        <CardHeader>
          <CardTitle>{t`Waiting to be thrown away`}</CardTitle>
          <CardDescription>
            {t`An image keeps its file while it is here, so a restored post still has its pictures. The file goes when the entry does.`}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {!entries ? (
            <DashboardLoading />
          ) : error ? (
            <DashboardError message={t`The bin could not be read just now.`} />
          ) : entries.length === 0 ? (
            <DashboardEmpty
              icon={Trash2}
              title={t`Nothing has been deleted`}
              description={t`When something is, it appears here first.`}
            />
          ) : (
            <ItemGroup className="rounded-xl border">
              {entries.map((entry) => (
                <Item key={entry.id} size="sm">
                  <ItemMedia>{icon(entry.kind)}</ItemMedia>
                  <ItemContent>
                    <ItemTitle>{entry.called}</ItemTitle>
                    <ItemDescription>
                      {named[entry.kind] ?? entry.kind} ·{" "}
                      {new Date(entry.thrown_away_at).toLocaleString()}
                    </ItemDescription>
                  </ItemContent>
                  <ItemActions>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={busy === entry.id}
                      onClick={() => restore(entry)}
                    >
                      <RotateCcw /> {t`Put it back`}
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t`Throw it away now`}
                      disabled={busy === entry.id}
                      onClick={() => purge(entry)}
                    >
                      <Trash2 />
                    </Button>
                  </ItemActions>
                </Item>
              ))}
            </ItemGroup>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
