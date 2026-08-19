import * as React from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"

import { calledIn } from "@/lib/kind-name"
import { Pencil, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { Content as Post } from "@api"
import {
  contentPublicationDate,
  contentStatus,
} from "@/lib/content"
import { useContentTypes } from "@/lib/use-content-types"
import { useLanguages } from "@/lib/use-languages"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Badge } from "@/components/ui/badge"
import { Checkbox } from "@/components/ui/checkbox"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { useStatusLabels } from "@/components/editor/types"
import {
  DashboardEmpty,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

/**
 * The list behind both Posts and Pages.
 *
 * They differ in one word and in nothing else — the same statuses, the same
 * languages, the same editor — so they are one screen told which kind it is
 * rather than two that would drift apart a fix at a time.
 */
export function ContentList({ kind }: { kind: string }) {
  const { t, i18n } = useLingui()
  const { find } = useContentTypes()
  const of_kind = find(kind)
  // Until the list of kinds arrives, the slug is a better label than nothing.
  const one = of_kind?.name ?? kind
  const many = of_kind ? calledIn(of_kind, i18n.locale, true) : kind
  const navigate = useNavigate()
  const STATUS_LABELS = useStatusLabels()
  const [posts, setPosts] = React.useState<Post[] | null>(null)
  const [going, setGoing] = React.useState<Post | null>(null)
  const [chosen, setChosen] = React.useState<Set<string>>(new Set())
  const { languages, defaultCode, label } = useLanguages()
  const [selectedLocale, setSelectedLocale] = React.useState("")
  const locale = selectedLocale || defaultCode

  const load = React.useCallback(() => {
    if (!locale) return

    every("content.list", { query: { kind, language: locale } })
      .then(setPosts)
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setPosts((held) => held ?? [])
      })
  }, [locale, kind])

  React.useEffect(() => load(), [load])

  const counts = React.useMemo(() => {
    const res = { draft: 0, scheduled: 0, published: 0, archived: 0 }
    if (!posts) return res
    for (const post of posts) {
      res[contentStatus(post)]++
    }
    return res
  }, [posts])

  /**
   * One act on everything ticked.
   */
  const actOnMany = async (act: string) => {
    try {
      for (const id of chosen) {
        if (act === "publish") {
          await api("content.publish", {
            path: { id },
          })
        } else if (act === "unpublish") {
          await api("content.update", {
            path: { id },
            body: { publication: "draft" },
          })
        } else if (act === "trash") {
          await api("content.trash", { path: { id } })
        }
      }

      setChosen(new Set())
      load()
      toast.success(t`Done.`)
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  const remove = async () => {
    if (!going) return

    try {
      await api("content.trash", { path: { id: going.id } })
      load()
      toast.success(t`${one} deleted`)
    } catch (why) {
      toast.error(apiMessage(why))
    } finally {
      setGoing(null)
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={many}
        description={t`Create, edit, and publish ${many.toLocaleLowerCase()}.`}
      />

      {languages.length > 1 && (
        <div className="mb-4 flex items-center gap-2">
          <span className="text-sm text-muted-foreground">{t`Language`}</span>
          <Select
            value={locale}
            onValueChange={(value) => {
              setSelectedLocale(value ?? "")
            }}
          >
            <SelectTrigger className="w-full max-w-52">
              <SelectValue>{(code: string) => label(code)}</SelectValue>
            </SelectTrigger>
            <SelectContent>
              {languages.map((language) => (
                <SelectItem key={language.tag} value={language.tag}>
                  {language.name} ({language.tag})
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}

      <div className="mb-6 grid grid-cols-2 gap-3 sm:grid-cols-2">
        {(["draft", "scheduled", "published", "archived"] as const).map(
          (status) => (
            <Card key={status}>
              <CardContent className="pt-6">
                <p className="text-2xl font-semibold">{counts[status]}</p>
                <p className="text-sm text-muted-foreground">
                  {STATUS_LABELS[status]}
                </p>
              </CardContent>
            </Card>
          )
        )}
      </div>

      {chosen.size > 0 && (
        <div className="mb-3 flex flex-wrap items-center gap-2 rounded-xl border border-border px-4 py-2">
          <span className="text-sm">{t`${chosen.size} ticked`}</span>
          <Button
            variant="outline"
            size="sm"
            onClick={() => void actOnMany("publish")}
          >
            {t`Publish them`}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => void actOnMany("unpublish")}
          >
            {t`Take them down`}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => void actOnMany("trash")}
          >
            {t`Throw them away`}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="ml-auto"
            onClick={() => setChosen(new Set())}
          >
            {t`Untick`}
          </Button>
        </div>
      )}

      {posts === null ? (
        <DashboardLoading />
      ) : posts.length === 0 ? (
        <DashboardEmpty
          title={t`No ${many} yet`}
          action={
            <Button
              onClick={() =>
                navigate({
                  to: "/editor/new",
                  search: { locale, translationOf: undefined, kind },
                })
              }
            >
              {t`New ${one}`}
            </Button>
          }
        />
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {posts.map((post) => (
            <div
              key={post.id}
              className="flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-3"
            >
              {/* The title takes the whole width on a phone and the status and
                  the two buttons drop below it, rather than squeezing a date
                  into the forty pixels left over. */}
              <Checkbox
                checked={chosen.has(post.id)}
                aria-label={t`Tick it`}
                onCheckedChange={(value) =>
                  setChosen((held) => {
                    const next = new Set(held)

                    if (value === true) {
                      next.add(post.id)
                    } else {
                      next.delete(post.id)
                    }

                    return next
                  })
                }
              />

              <div className="min-w-0 basis-full sm:flex-1 sm:basis-0">
                <Link
                  to="/editor/$postId"
                  params={{ postId: post.id }}
                  className="block truncate text-sm font-medium hover:underline"
                >
                  {post.title || t`Untitled`}
                </Link>
                <p className="truncate text-xs text-muted-foreground">
                  {new Date(contentPublicationDate(post)).toLocaleString(
                    i18n.locale,
                    {
                      dateStyle: "medium",
                      timeStyle: "short",
                    }
                  )}
                </p>
              </div>
              <Badge
                variant={
                  contentStatus(post) === "published" ? "default" : "secondary"
                }
                className="ml-auto sm:ml-0"
              >
                {STATUS_LABELS[contentStatus(post)]}
              </Badge>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Edit`}
                onClick={() =>
                  navigate({
                    to: "/editor/$postId",
                    params: { postId: post.id },
                  })
                }
              >
                <Pencil />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Delete`}
                onClick={() => setGoing(post)}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      <AlertDialog
        open={going !== null}
        onOpenChange={(open) => !open && setGoing(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Delete this ${one}?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`"${going?.title}" goes to the bin, and can be taken back out of it.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void remove()}>
              {t`Delete`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
