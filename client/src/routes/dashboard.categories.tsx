/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
  Check,
  CornerDownRight,
  Loader2,
  Pencil,
  Plus,
  Trash2,
  X,
} from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Term as Category } from "@api"
import { useLanguages } from "@/lib/use-languages"
import { descendantsOf, toCategoryTree } from "@/lib/category-tree"
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

export const Route = createFileRoute("/dashboard/categories")({
  component: CategoriesRoute,
})

const NO_PARENT = "__none__"

function CategoriesRoute() {
  const { t } = useLingui()
  const { languages, defaultCode, label } = useLanguages()
  const [selectedLocale, setSelectedLocale] = React.useState("")
  const locale = selectedLocale || defaultCode

  const [categories, setCategories] = React.useState<Category[] | null>(null)
  const [name, setName] = React.useState("")
  const [parentId, setParentId] = React.useState<string>(NO_PARENT)
  const [editing, setEditing] = React.useState<Category | null>(null)
  const [editName, setEditName] = React.useState("")
  const [editParent, setEditParent] = React.useState<string>(NO_PARENT)
  const [pendingDelete, setPendingDelete] = React.useState<Category | null>(
    null
  )

  const load = React.useCallback(() => {
    if (!locale) return
    every("GET /api/terms", { query: { sort: "category", language: locale } })
      .then((terms) => setCategories(terms.filter((t) => t.sort === "category")))
      .catch((why: unknown) => {
        toast.error(said(why))
        setCategories((held) => held ?? [])
      })
  }, [locale])

  React.useEffect(() => load(), [load])

  const rows = React.useMemo(
    () => toCategoryTree(categories ?? []),
    [categories]
  )

  const create = async () => {
    const value = name.trim()
    if (!value) return
    try {
      await api("POST /api/terms", {
        body: {
          sort: "category",
          language: locale,
          slug: value.toLowerCase().replace(/[^a-z0-9]+/g, "-"),
          name: value,
          parent: parentId === NO_PARENT ? null : parentId,
        },
      })
      setName("")
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const startEditing = (category: Category) => {
    setEditing(category)
    setEditName(category.name)
    setEditParent(category.parent ?? NO_PARENT)
  }

  const saveEdit = async () => {
    if (!editing) return
    const value = editName.trim()
    if (!value) return
    try {
      await api("PATCH /api/terms/{id}", {
        path: { id: editing.id },
        body: {
          name: value,
          parent: editParent === NO_PARENT ? null : editParent,
        },
      })
      setEditing(null)
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const confirmDelete = async () => {
    if (!pendingDelete) return
    try {
      await api("DELETE /api/terms/{id}", { path: { id: pendingDelete.id } })
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setPendingDelete(null)
    }
  }

  const parentLabel = React.useCallback(
    (id: string) =>
      id === NO_PARENT
        ? t`No parent (top level)`
        : (categories?.find((category) => category.id === id)?.name ?? id),
    [categories, t]
  )

  const parentOptions = (exclude?: string) => {
    const blocked = exclude
      ? descendantsOf(exclude, categories ?? [])
      : new Set<string>()
    return rows.filter((row) => !blocked.has(row.category.id))
  }

  return (
    <>
      <div className="mb-6 flex items-center gap-3">
        <h1 className="text-lg font-semibold">{t`Categories`}</h1>
        {languages.length > 1 && (
          <Select
            value={locale}
            onValueChange={(value) => setSelectedLocale(value ?? "")}
          >
            <SelectTrigger className="w-44">
              <SelectValue>{(code: string) => label(code)}</SelectValue>
            </SelectTrigger>
            <SelectContent>
              {languages.map((language) => (
                <SelectItem key={language.tag} value={language.tag}>
                  {language.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </div>

      <form
        onSubmit={(event) => {
          event.preventDefault()
          void create()
        }}
        className="mb-6 flex flex-wrap gap-2"
      >
        <Input
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder={t`New category`}
          className="max-w-xs"
        />
        <Select
          value={parentId}
          onValueChange={(value) => setParentId(value ?? NO_PARENT)}
        >
          <SelectTrigger className="w-60">
            <SelectValue>{(id: string) => parentLabel(id)}</SelectValue>
          </SelectTrigger>
          <SelectContent>
            <SelectItem
              value={NO_PARENT}
            >{t`No parent (top level)`}</SelectItem>
            {parentOptions().map((row) => (
              <SelectItem key={row.category.id} value={row.category.id}>
                {" ".repeat(row.depth * 3)}
                {row.category.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button type="submit" disabled={!name.trim()}>
          <Plus /> {t`Add category`}
        </Button>
      </form>

      {categories === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : rows.length === 0 ? (
        <p className="py-16 text-center text-sm text-muted-foreground">
          {t`No categories yet`}
        </p>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {rows.map(({ category, depth }) => (
            <div
              key={category.id}
              className="flex items-center gap-3 px-4 py-3"
            >
              <div
                className="flex min-w-0 flex-1 items-center gap-2"
                style={{ paddingLeft: `${depth * 1.5}rem` }}
              >
                {depth > 0 && (
                  <CornerDownRight className="size-3.5 shrink-0 text-muted-foreground" />
                )}
                {editing?.id === category.id ? (
                  <div className="flex flex-wrap items-center gap-2">
                    <Input
                      value={editName}
                      onChange={(event) => setEditName(event.target.value)}
                      className="h-8 max-w-56"
                      autoFocus
                    />
                    <Select
                      value={editParent}
                      onValueChange={(value) =>
                        setEditParent(value ?? NO_PARENT)
                      }
                    >
                      <SelectTrigger size="sm" className="w-52">
                        <SelectValue>
                          {(id: string) => parentLabel(id)}
                        </SelectValue>
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem
                          value={NO_PARENT}
                        >{t`No parent (top level)`}</SelectItem>
                        {parentOptions(category.id).map((row) => (
                          <SelectItem
                            key={row.category.id}
                            value={row.category.id}
                          >
                            {" ".repeat(row.depth * 3)}
                            {row.category.name}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                ) : (
                  <div className="min-w-0">
                    <p className="truncate text-sm font-medium">
                      {category.name}
                    </p>
                    <p className="truncate text-xs text-muted-foreground">
                      {category.slug}
                    </p>
                  </div>
                )}
              </div>

              {editing?.id === category.id ? (
                <>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t`Save`}
                    onClick={() => void saveEdit()}
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
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t`Edit`}
                    onClick={() => startEditing(category)}
                  >
                    <Pencil />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t`Delete`}
                    onClick={() => setPendingDelete(category)}
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
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Delete this category?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`"${pendingDelete?.name}" will be permanently deleted. This cannot be undone.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void confirmDelete()}>
              {t`Delete`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
