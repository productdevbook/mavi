import * as React from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"

import { calledIn } from "@/lib/kind-name"
import { Loader2, Pencil, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Counts, Post, State } from "@api"
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
  const [counted, setCounted] = React.useState<Counts | null>(null)
  const [going, setGoing] = React.useState<Post | null>(null)
  const [chosen, setChosen] = React.useState<Set<string>>(new Set())
  const { languages, defaultCode, label } = useLanguages()
  const [selectedLocale, setSelectedLocale] = React.useState("")
  // Named rather than counted, and all of them: a post in two categories
  // showed one, which read as though it were only in that one.
  // Counts and the list are always scoped to one language: totalling every
  // language would report "36 posts" for 12 posts in three languages. Derived
  // rather than synced through an effect so the default landing doesn't cause
  // a cascading render.
  const locale = selectedLocale || defaultCode

  // Which kind: `post` and `page` are what a post is; anything else is a kind
  // the site added, which is a `type` rather than a kind.
  const asking =
    kind === "post" || kind === "page"
      ? { kind, language: locale }
      : { type: kind, language: locale }

  const load = React.useCallback(() => {
    if (!locale) return

    every("GET /api/posts", { query: asking })
      .then(setPosts)
      .catch((why: unknown) => {
        toast.error(said(why))
        setPosts((held) => held ?? [])
      })

    api("GET /api/posts/counts", { query: asking })
      .then(setCounted)
      .catch((why: unknown) => toast.error(said(why)))
    // The query is rebuilt every render; what it depends on is these two.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [locale, kind])

  React.useEffect(() => load(), [load])

  // Straight from the server: counting the rows on screen would report the
  // size of the page rather than the archive.
  const counts: Record<string, number> = counted
    ? {
        draft: counted.draft,
        scheduled: counted.scheduled,
        published: counted.published,
        archived: counted.archived,
      }
    : { draft: 0, scheduled: 0, published: 0, archived: 0 }

  /**
   * One act on everything ticked.
   *
   * What is left alone comes back named rather than counted: a person who may
   * change their own and ticked twenty needs to know which four did not move.
   */
  const actOnMany = async (act: string) => {
    try {
      const done = await api("POST /api/posts/actions", {
        body: { act, ids: [...chosen] },
      })

      setChosen(new Set())
      load()

      if (done.left_alone.length > 0) {
        toast.warning(
          t`${done.acted_on} done. ${done.left_alone.length} were left alone.`,
        )
      } else {
        toast.success(t`${done.acted_on} done.`)
      }
    } catch (why) {
      toast.error(said(why))
    }
  }

  const remove = async () => {
    if (!going) return

    try {
      await api("DELETE /api/posts/{id}", { path: { id: going.id } })
      // Reloaded rather than filtered out: the counts and the list below them
      // both move when a post goes.
      load()
      toast.success(t`${one} deleted`)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setGoing(null)
    }
  }

  return (
    <>
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
                <SelectItem key={language.code} value={language.code}>
                  {language.name} ({language.code})
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}

      <div className="mb-6 grid grid-cols-2 gap-3 sm:grid-cols-4">
        {(Object.keys(counts) as State[]).map((status) => (
          <Card key={status}>
            <CardContent className="pt-6">
              <p className="text-2xl font-semibold">{counts[status]}</p>
              <p className="text-sm text-muted-foreground">
                {STATUS_LABELS[status]}
              </p>
            </CardContent>
          </Card>
        ))}
      </div>

      {chosen.size > 0 && (
        <div className="mb-3 flex flex-wrap items-center gap-2 rounded-xl border border-border px-4 py-2">
          <span className="text-sm">{t`${chosen.size} ticked`}</span>
          <Button variant="outline" size="sm" onClick={() => void actOnMany("publish")}>
            {t`Publish them`}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => void actOnMany("unpublish")}
          >
            {t`Take them down`}
          </Button>
          <Button variant="outline" size="sm" onClick={() => void actOnMany("trash")}>
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
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : posts.length === 0 ? (
        <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-border py-16 text-center">
          <p className="text-sm text-muted-foreground">{t`No ${many} yet`}</p>
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
        </div>
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
                  {new Date(post.published_at ?? post.created_at).toLocaleString(
                    i18n.locale,
                    { dateStyle: "medium", timeStyle: "short" },
                  )}
                </p>
              </div>
              <Badge
                variant={post.state === "published" ? "default" : "secondary"}
                className="ml-auto sm:ml-0"
              >
                {STATUS_LABELS[post.state]}
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
    </>
  )
}
