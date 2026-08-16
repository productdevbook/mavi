/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Check, Loader2, Pencil, Plus, Trash2, X } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { useLanguages } from "@/lib/use-languages"
import type { Term } from "@api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
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

export const Route = createFileRoute("/dashboard/tags")({
  component: TagsRoute,
})

/**
 * The tags this site files things under.
 *
 * One language at a time: a tag belongs to the language it was written in, and
 * a site that writes in two has two lists rather than one list with a column
 * per language.
 */
function TagsRoute() {
  const { t } = useLingui()
  const { languages, defaultCode } = useLanguages()
  const [chosen, setChosen] = React.useState("")
  const language = chosen || defaultCode

  const [tags, setTags] = React.useState<Term[] | null>(null)
  const [name, setName] = React.useState("")
  const [editing, setEditing] = React.useState<Term | null>(null)
  const [editName, setEditName] = React.useState("")
  const [going, setGoing] = React.useState<Term | null>(null)

  const load = React.useCallback(() => {
    if (!language) return

    every("GET /api/terms", { query: { kind: "tag", language } })
      .then(setTags)
      .catch((why: unknown) => {
        toast.error(said(why))
        setTags((held) => held ?? [])
      })
  }, [language])

  React.useEffect(() => load(), [load])

  const add = async () => {
    const wanted = name.trim()

    if (!wanted) return

    try {
      await api("POST /api/terms", {
        body: { kind: "tag", language, name: wanted },
      })
      setName("")
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const rename = async () => {
    if (!editing || !editName.trim()) return

    try {
      await api("PATCH /api/terms/{id}", {
        path: { id: editing.id },
        body: { name: editName.trim() },
      })
      setEditing(null)
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const remove = async () => {
    if (!going) return

    try {
      await api("DELETE /api/terms/{id}", { path: { id: going.id } })
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setGoing(null)
    }
  }

  return (
    <>
      <div className="mb-6 flex items-center gap-3">
        <h1 className="text-lg font-semibold">{t`Tags`}</h1>
        {languages.length > 1 && (
          <Select
            value={language}
            onValueChange={(value) => setChosen(value ?? "")}
          >
            <SelectTrigger className="w-44">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {languages.map((one) => (
                <SelectItem key={one.code} value={one.code}>
                  {one.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </div>

      <form
        onSubmit={(event) => {
          event.preventDefault()
          void add()
        }}
        className="mb-6 flex max-w-xl gap-2"
      >
        <Input
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder={t`A new tag`}
        />
        <Button type="submit" disabled={!name.trim()}>
          <Plus /> {t`Add`}
        </Button>
      </form>

      {tags === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : tags.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border py-16 text-center text-sm text-muted-foreground">
          {t`No tags yet.`}
        </p>
      ) : (
        <div className="flex max-w-xl flex-col divide-y divide-border rounded-xl border border-border">
          {tags.map((tag) => (
            <div
              key={tag.id}
              className="flex items-center gap-3 px-4 py-2.5"
            >
              {editing?.id === tag.id ? (
                <>
                  <Input
                    autoFocus
                    value={editName}
                    onChange={(event) => setEditName(event.target.value)}
                    className="h-8"
                  />
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t`Save`}
                    onClick={() => void rename()}
                  >
                    <Check />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t`Cancel`}
                    onClick={() => setEditing(null)}
                  >
                    <X />
                  </Button>
                </>
              ) : (
                <>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium">{tag.name}</p>
                    <p className="truncate text-xs text-muted-foreground">
                      {tag.slug}
                    </p>
                  </div>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t`Rename`}
                    onClick={() => {
                      setEditing(tag)
                      setEditName(tag.name)
                    }}
                  >
                    <Pencil />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t`Delete`}
                    onClick={() => setGoing(tag)}
                  >
                    <Trash2 />
                  </Button>
                </>
              )}
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
            <AlertDialogTitle>{t`Remove this tag?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`What is filed under it stays; it stops being filed under this.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void remove()}>
              {t`Remove`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
