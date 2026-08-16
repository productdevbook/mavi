/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
  FileText,
  Image as ImageIcon,
  Inbox,
  Mails,
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
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from "@/components/ui/item"
import { Spinner } from "@/components/ui/spinner"

export const Route = createFileRoute("/dashboard/trash")({
  component: TrashRoute,
})

function TrashRoute() {
  const { t } = useLingui()

  const [entries, setEntries] = React.useState<Thrown[] | null>(null)
  const [busy, setBusy] = React.useState<string | null>(null)

  const load = React.useCallback(() => {
    inTheBin()
      .then(setEntries)
      .catch((why: unknown) => {
        toast.error(said(why))
        setEntries([])
      })
  }, [])

  React.useEffect(load, [load])

  // v1 names the table a thing came out of, which is what putting it back
  // needs to know.
  const named: Record<string, string> = {
    posts: t`post`,
    media: t`image`,
    videos: t`video`,
    forms: t`form`,
    form_submissions: t`what somebody sent`,
    terms: t`category or tag`,
    mail_lists: t`mailing list`,
    campaigns: t`campaign`,
    courses: t`course`,
    boards: t`board`,
    cards: t`card`,
    products: t`product`,
    flows: t`flow`,
  }

  const icon = (kind: string) => {
    const glyph =
      kind === "media" || kind === "videos"
        ? ImageIcon
        : kind === "forms" || kind === "form_submissions"
          ? Inbox
          : kind === "terms"
            ? Tags
            : kind.startsWith("mail_") || kind === "campaigns"
              ? Mails
              : FileText
    return React.createElement(glyph, {
      className: "size-4 text-muted-foreground",
    })
  }

  const restore = (entry: Thrown) => {
    setBusy(entry.id)
    putBack(entry.kind, entry.id)
      .then(() => {
        toast.success(t`${entry.name} is back`)
        load()
      })
      .catch((why) => toast.error(said(why)))
      .finally(() => setBusy(null))
  }

  const purge = (entry: Thrown) => {
    setBusy(entry.id)
    forGood(entry.kind, entry.id)
      .then(load)
      .catch((why) =>
        toast.error(
          said(why)
        )
      )
      .finally(() => setBusy(null))
  }

  return (
    <>
      <div className="mb-6">
        <h1 className="text-lg font-semibold">{t`Bin`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`Nothing deleted here goes straight away. It waits thirty days, whether a person deleted it or an assistant did, and until then it can be put back exactly as it was.`}
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>{t`Waiting to be thrown away`}</CardTitle>
          <CardDescription>
            {t`An image keeps its file while it is here, so a restored post still has its pictures. The file goes when the entry does.`}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {!entries ? (
            <div className="flex justify-center py-8">
              <Spinner className="size-5 text-muted-foreground" />
            </div>
          ) : entries.length === 0 ? (
            <Empty className="border">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <Trash2 />
                </EmptyMedia>
                <EmptyTitle>{t`Nothing has been deleted`}</EmptyTitle>
                <EmptyDescription>
                  {t`When something is, it appears here first.`}
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : (
            <ItemGroup className="rounded-xl border">
              {entries.map((entry) => (
                <Item key={entry.id} size="sm">
                  <ItemMedia>{icon(entry.kind)}</ItemMedia>
                  <ItemContent>
                    <ItemTitle>{entry.name}</ItemTitle>
                    <ItemDescription>
                      {named[entry.kind] ?? entry.kind} ·{" "}
                      {new Date(entry.thrown_at).toLocaleString()}
                    </ItemDescription>
                    <ItemDescription>
                      {t`Goes for good on ${new Date(entry.goes_at).toLocaleDateString()}`}
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
    </>
  )
}
