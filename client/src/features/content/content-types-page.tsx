import * as React from "react"
import { useLingui } from "@lingui/react/macro"

import { calledIn } from "@/lib/kind-name"
import { Plus, Shapes, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { ContentType } from "@/lib/use-content-types"
import { useContentTypes } from "@/lib/use-content-types"
import { Button } from "@/components/ui/button"
import { ContentTypeEditor } from "@/components/dashboard/content-type-editor"
import {
  DashboardEmpty,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

/**
 * What this site publishes.
 *
 * A blog has posts and pages. A training company also has courses, and a
 * course has a price and a level — facts a front end can lay out, rather than
 * numbers typed into a paragraph where nothing can find them. This is where a
 * site says what its own kinds of thing are made of.
 */
export function ContentTypesPage() {
  const { t, i18n } = useLingui()
  const { types, loading, reload } = useContentTypes()
  const [editing, setEditing] = React.useState<ContentType | null>(null)
  const [adding, setAdding] = React.useState(false)

  const remove = async (kind: ContentType) => {
    try {
      await api("kinds.stop-saying", { path: { kind: kind.kind } })
      reload()
    } catch (why) {
      toast.error(said(why))
    }
  }

  if (editing || adding) {
    return (
      <ContentTypeEditor
        kind={editing}
        onDone={() => {
          setEditing(null)
          setAdding(false)
          reload()
        }}
      />
    )
  }

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`What this site publishes`}
        description={t`Every site has posts and pages. Add a kind of your own when what you publish has facts of its own — a course with a price and a level, a property with rooms — so a page can lay them out instead of hunting for them in a paragraph.`}
        actions={
          <Button onClick={() => setAdding(true)}>
            <Plus /> {t`Add a kind`}
          </Button>
        }
      />

      {loading ? (
        <DashboardLoading />
      ) : types.length === 0 ? (
        <DashboardEmpty
          icon={Shapes}
          title={t`No custom kinds yet`}
          description={t`Add a kind when your site publishes something with facts of its own.`}
          action={
            <Button onClick={() => setAdding(true)}>{t`Add a kind`}</Button>
          }
        />
      ) : (
        <div className="flex max-w-3xl flex-col divide-y divide-border rounded-xl border border-border">
          {types.map((kind) => (
            <div
              key={kind.kind}
              className="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-3"
            >
              <Shapes className="size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1 basis-40">
                <p className="truncate text-sm font-medium">
                  {calledIn(kind, i18n.locale, true)}
                </p>
                <p className="truncate text-xs text-muted-foreground">
                  {kind.kind} ·{" "}
                  {!kind.fields || kind.fields.length === 0
                    ? t`no fields of its own`
                    : kind.fields.map((field) => field.label).join(", ")}
                </p>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => setEditing(kind)}
              >
                {t`Fields`}
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Remove`}
                onClick={() => void remove(kind)}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
