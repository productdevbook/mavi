import * as React from "react"
import { useLingui } from "@lingui/react/macro"

import { calledIn } from "@/lib/kind-name"

import { ImageOff, Plus, Sparkles, Upload, X } from "lucide-react"
import { toast } from "sonner"

import { cn } from "@/lib/utils"
import { slugify } from "@/lib/editor-utils"
import { api, every, Refused } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Term } from "@api"
import { toCategoryTree } from "@/lib/category-tree"
import type { ContentType } from "@/lib/use-content-types"
import { Badge } from "@/components/ui/badge"
import { Checkbox } from "@/components/ui/checkbox"
import { useContentTypes } from "@/lib/use-content-types"
import { ContentFields, type Field } from "@/components/editor/content-fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import {
  useStatusLabels,
  type PostMeta,
  type PostStatus,
} from "@/components/editor/types"

interface PostSettingsProps {
  meta: PostMeta
  /** Null until the post has been saved once: what is wrong with a post is
      worked out when it is written, so there is nothing to read before then. */
  postId: string | null
  onChange: (patch: Partial<PostMeta>) => void
  /** Categories and tags are offered only in the post's own language. */
  locale: string
  plainText: string
}

function declared(kind: ContentType): Field[] {
  const fields = kind.fields ?? []

  return fields.map((field) => ({
    name: field.key,
    label: field.label || field.key,
    type:
      field.kind === "choice"
        ? "select"
        : field.kind === "boolean"
          ? "checkbox"
          : field.kind,
    required: field.required ?? false,
    options: field.options ?? [],
    fields: [],
    role: "",
  }))
}

export function PostSettings({
  meta,
  postId: _postId,
  onChange,
  locale,
  plainText,
}: PostSettingsProps) {
  const { t, i18n } = useLingui()
  const [tagDraft, setTagDraft] = React.useState("")
  const [categories, setCategories] = React.useState<Term[]>([])
  const categoryRows = React.useMemo(
    () => toCategoryTree(categories),
    [categories]
  )
  const [tags, setTags] = React.useState<Term[]>([])
  const [newCategory, setNewCategory] = React.useState("")
  const coverInputRef = React.useRef<HTMLInputElement>(null)

  const STATUS_LABELS = useStatusLabels()

  React.useEffect(() => {
    if (!locale) return
    every("GET /api/terms", { query: { sort: "category", language: locale } })
      .then((terms) => setCategories(terms.filter((t) => t.sort === "category")))
      .catch(() => setCategories([]))

    every("GET /api/terms", { query: { sort: "tag", language: locale } })
      .then((terms) => setTags(terms.filter((t) => t.sort === "tag")))
      .catch(() => setTags([]))
  }, [locale])

  const addCategory = async () => {
    const name = newCategory.trim()
    if (!name) return
    try {
      const created = await api("POST /api/terms", {
        body: {
          sort: "category",
          language: locale,
          slug: slugify(name),
          name,
        },
      })

      setCategories((held) =>
        held.some((category) => category.id === created.id)
          ? held
          : [...held, created],
      )
      onChange(withCategory(created.id, true))
      setNewCategory("")
    } catch (why) {
      toast.error(said(why))
    }
  }

  const addTag = async () => {
    const value = tagDraft.trim()
    if (!value || meta.tags.includes(value)) return
    setTagDraft("")
    onChange({ tags: [...meta.tags, value] })
    try {
      const created = await api("POST /api/terms", {
        body: {
          sort: "tag",
          language: locale,
          slug: slugify(value),
          name: value,
        },
      })

      setTags((held) =>
        held.some((tag) => tag.id === created.id) ? held : [...held, created],
      )
    } catch (why) {
      toast.error(said(why))
    }
  }

  // What this thing is, and so which fields it has beyond the usual ones.
  const { find } = useContentTypes()
  const of_kind = find(meta.kind)

  /**
   * The post's categories after one is put in or taken out.
   *
   * `category` is the name of one of them, kept because a front end has read
   * it since before there were several. The first is the one it names.
   */
  const withCategory = (id: string, wanted: boolean) => {
    const ids = wanted
      ? [...meta.categoryIds.filter((held) => held !== id), id]
      : meta.categoryIds.filter((held) => held !== id)
    const first = categories.find((category) => category.id === ids[0])
    return { categoryIds: ids, category: first?.name ?? "" }
  }

  const seoTitle = meta.seoTitle || meta.title
  const seoDescription = meta.seoDescription || meta.excerpt

  return (
    <div className="flex flex-col gap-5">
      {of_kind && declared(of_kind).length > 0 && (
        <div className="flex flex-col gap-5 rounded-xl border border-border px-4 py-4">
          <p className="text-sm font-medium">
            {calledIn(of_kind, i18n.locale)}
          </p>
          <ContentFields
            fields={declared(of_kind)}
            values={meta.fields}
            onChange={(fields) => onChange({ fields })}
          />
        </div>
      )}

      <Field label={t`Status`} htmlFor="meta-status">
        <Select
          value={meta.status}
          onValueChange={(value) => onChange({ status: value as PostStatus })}
        >
          <SelectTrigger id="meta-status" className="w-full">
            <SelectValue>
              {(value: PostStatus | null) =>
                value ? STATUS_LABELS[value] : t`Select`
              }
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {Object.entries(STATUS_LABELS).map(([value, label]) => (
              <SelectItem key={value} value={value}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>

      <Field
        label={t`Publish date`}
        htmlFor="meta-date"
        hint={
          meta.status === "scheduled" && !meta.publishAt
            ? t`Needed to schedule`
            : undefined
        }
      >
        <Input
          id="meta-date"
          type="datetime-local"
          value={meta.publishAt}
          onChange={(event) => onChange({ publishAt: event.target.value })}
        />
      </Field>

      <Field
        label={t`Permalink`}
        htmlFor="meta-slug"
        hint={`/blog/${meta.slug || t`post-url`}`}
      >
        <div className="flex gap-1.5">
          <Input
            id="meta-slug"
            value={meta.slug}
            onChange={(event) => onChange({ slug: event.target.value })}
            placeholder={t`post-url`}
          />
          <Button
            variant="outline"
            size="icon"
            aria-label={t`Generate from title`}
            onClick={() => {
              onChange({ slug: slugify(meta.title) })
            }}
          >
            <Sparkles />
          </Button>
        </div>
      </Field>

      <Field label={t`Categories`} htmlFor="meta-category">
        {/* Every one it is in, not the first. A post has always been able to
            be in several — the joining table is what carries them — and this
            was a single choice, so opening a post with two and saving it
            deleted the second without saying so. */}
        <div className="flex max-h-56 flex-col gap-0.5 overflow-y-auto rounded-md border border-input p-1.5">
          {categoryRows.length === 0 ? (
            <p className="px-1.5 py-1 text-sm text-muted-foreground">
              {t`No categories yet.`}
            </p>
          ) : (
            categoryRows.map(({ category, depth }) => (
              <Label
                key={category.id}
                className="flex items-center gap-2 rounded px-1.5 py-1 font-normal hover:bg-muted"
                style={{ paddingLeft: `${0.375 + depth * 1}rem` }}
              >
                <Checkbox
                  checked={meta.categoryIds.includes(category.id)}
                  onCheckedChange={(checked: boolean | "indeterminate") =>
                    onChange(withCategory(category.id, checked === true))
                  }
                />
                {category.name}
              </Label>
            ))
          )}
        </div>
        <div className="flex gap-1.5">
          <Input
            value={newCategory}
            placeholder={t`New category`}
            onChange={(event) => setNewCategory(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault()
                void addCategory()
              }
            }}
          />
          <Button
            type="button"
            variant="outline"
            size="icon"
            aria-label={t`Add category`}
            onClick={() => void addCategory()}
          >
            <Plus />
          </Button>
        </div>
      </Field>

      <Field label={t`Tags`} htmlFor="meta-tags">
        <div className="flex gap-1.5">
          <Input
            id="meta-tags"
            value={tagDraft}
            list="meta-tags-suggestions"
            placeholder={t`Add a tag and press Enter`}
            onChange={(event) => setTagDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault()
                void addTag()
              }
            }}
          />
          <datalist id="meta-tags-suggestions">
            {tags.map((tag) => (
              <option key={tag.id} value={tag.name} />
            ))}
          </datalist>
        </div>
        {meta.tags.length > 0 && (
          <div className="flex flex-wrap gap-1.5 pt-2">
            {meta.tags.map((tag) => (
              <Badge key={tag} variant="secondary" className="gap-1 pr-1">
                {tag}
                <button
                  type="button"
                  aria-label={t`Remove ${tag} tag`}
                  onClick={() =>
                    onChange({ tags: meta.tags.filter((item) => item !== tag) })
                  }
                  className="rounded-full p-0.5 hover:bg-foreground/10"
                >
                  <X className="size-3" />
                </button>
              </Badge>
            ))}
          </div>
        )}
      </Field>

      <Field
        label={t`Excerpt`}
        htmlFor="meta-excerpt"
        hint={`${meta.excerpt.length}/160`}
      >
        <Textarea
          id="meta-excerpt"
          value={meta.excerpt}
          maxLength={220}
          rows={3}
          placeholder={t`Short description shown on listing pages`}
          onChange={(event) => onChange({ excerpt: event.target.value })}
          className="resize-none"
        />
        <Button
          variant="ghost"
          size="sm"
          className="mt-1 self-start text-muted-foreground"
          onClick={() =>
            onChange({ excerpt: plainText.slice(0, 155).trim() + "…" })
          }
        >
          <Sparkles /> {t`Generate from post`}
        </Button>
      </Field>

      <Field label={t`Cover image`} htmlFor="meta-cover">
        <div className="flex gap-1.5">
          <Input
            id="meta-cover"
            value={meta.coverUrl}
            placeholder="https://…/cover.jpg"
            onChange={(event) => onChange({ coverUrl: event.target.value })}
          />
          <Button
            type="button"
            variant="outline"
            size="icon"
            aria-label={t`Upload cover image`}
            onClick={() => coverInputRef.current?.click()}
          >
            <Upload />
          </Button>
          <input
            ref={coverInputRef}
            type="file"
            accept="image/png,image/jpeg,image/gif,image/webp"
            hidden
            onChange={async (event) => {
              const file = event.target.files?.[0]
              event.target.value = ""
              if (!file) return
              const response = await fetch(
                `/api/files?name=${encodeURIComponent(file.name)}`,
                { method: "POST", body: file },
              )

              if (!response.ok) {
                const why = await response.json().catch(() => null)

                toast.error(
                  said(
                    new Refused(
                      response.status,
                      String(why?.error?.code ?? "internal"),
                      why?.error?.key ?? null,
                      why?.error?.named ?? {},
                      String(why?.error?.message ?? response.statusText),
                    ),
                  ),
                )

                return
              }

              const media = (await response.json()) as { id: string }

              onChange({ coverId: media.id, coverUrl: `/uploads/${media.id}` })
            }}
          />
        </div>
        <div className="mt-2 flex aspect-video items-center justify-center overflow-hidden rounded-lg border border-dashed border-border bg-muted/40">
          {meta.coverUrl ? (
            <img
              src={meta.coverUrl}
              alt={t`Cover preview`}
              className="size-full object-cover"
            />
          ) : (
            <ImageOff className="size-5 text-muted-foreground" />
          )}
        </div>
      </Field>

      <div className="flex flex-col gap-4 rounded-xl border border-border p-3">
        <p className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
          {t`Search engine`}
        </p>
        <Field
          label={t`SEO title`}
          htmlFor="meta-seo-title"
          hint={`${seoTitle.length}/60`}
        >
          <Input
            id="meta-seo-title"
            value={meta.seoTitle}
            placeholder={meta.title}
            onChange={(event) => onChange({ seoTitle: event.target.value })}
          />
        </Field>
        <Field
          label={t`SEO description`}
          htmlFor="meta-seo-description"
          hint={`${seoDescription.length}/160`}
        >
          <Textarea
            id="meta-seo-description"
            rows={3}
            value={meta.seoDescription}
            placeholder={meta.excerpt || t`Text shown in search results`}
            onChange={(event) =>
              onChange({ seoDescription: event.target.value })
            }
            className="resize-none"
          />
        </Field>
        <Field label={t`Canonical URL`} htmlFor="meta-canonical">
          <Input
            id="meta-canonical"
            value={meta.canonical}
            placeholder="https://example.com/blog/…"
            onChange={(event) => onChange({ canonical: event.target.value })}
          />
        </Field>

        <div className="rounded-lg border border-border bg-muted/40 p-3">
          <p className="truncate text-xs text-muted-foreground">
            example.com › blog › {meta.slug || t`post`}
          </p>
          <p className="truncate text-sm font-medium text-primary">
            {seoTitle || t`Post title`}
          </p>
          <p className="line-clamp-2 text-xs text-muted-foreground">
            {seoDescription || t`Description text shown in search results.`}
          </p>
        </div>
      </div>
    </div>
  )
}

function Field({
  label,
  htmlFor,
  hint,
  children,
  className,
}: {
  label: string
  htmlFor: string
  hint?: string
  children: React.ReactNode
  className?: string
}) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <div className="flex items-baseline justify-between gap-2">
        <Label htmlFor={htmlFor}>{label}</Label>
        {hint && (
          <span className="truncate text-[0.7rem] text-muted-foreground">
            {hint}
          </span>
        )}
      </div>
      {children}
    </div>
  )
}

