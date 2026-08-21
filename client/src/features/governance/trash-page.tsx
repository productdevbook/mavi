import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import {
  FileText,
  GraduationCap,
  Image as ImageIcon,
  Inbox,
  ShoppingBag,
  Tag,
  RotateCcw,
  Tags,
  Trash2,
  UserRound,
} from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { TrashItem } from "@api"
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

  const [entries, setEntries] = React.useState<TrashItem[] | null>(null)
  const [busy, setBusy] = React.useState<string | null>(null)
  const [error, setError] = React.useState(false)

  const load = React.useCallback(() => {
    setError(false)
    every("trash.items.list", { query: {} })
      .then(setEntries)
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setError(true)
        setEntries([])
      })
  }, [])

  React.useEffect(load, [load])

  const named: Record<string, string> = {
    course: t`course`,
    student: t`student`,
    form: t`form`,
    product: t`product`,
    coupon: t`coupon`,
    content: t`post`,
    file: t`file`,
    term: t`category or tag`,
  }

  const icon = (kind: string) => {
    const glyph =
      kind === "file"
        ? ImageIcon
        : kind === "course"
          ? GraduationCap
          : kind === "student"
            ? UserRound
            : kind === "product"
              ? ShoppingBag
              : kind === "coupon"
                ? Tag
                : kind === "content"
                  ? Inbox
                  : kind === "term"
                    ? Tags
                    : FileText
    return React.createElement(glyph, {
      className: "size-4 text-muted-foreground",
    })
  }

  const restore = (entry: TrashItem) => {
    setBusy(entry.id)
    api("trash.items.restore", { path: { kind: entry.kind, id: entry.id } })
      .then(() => {
        toast.success(t`${entry.label} is back`)
        load()
      })
      .catch((why) => toast.error(apiMessage(why)))
      .finally(() => setBusy(null))
  }

  const purge = (entry: TrashItem) => {
    setBusy(entry.id)
    api("trash.items.delete_permanently", {
      path: { kind: entry.kind, id: entry.id },
    })
      .then(load)
      .catch((why) => toast.error(apiMessage(why)))
      .finally(() => setBusy(null))
  }

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`Bin`}
        description={t`Deleted resources wait thirty days before they are removed permanently, and can be restored in the meantime.`}
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
                    <ItemTitle>{entry.label}</ItemTitle>
                    <ItemDescription>
                      {named[entry.kind] ?? entry.kind} ·{" "}
                      {new Date(entry.deleted_at).toLocaleString()}
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
