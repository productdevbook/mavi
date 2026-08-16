import { useLingui } from "@lingui/react/macro"

/** What a post's state may be, as the API keeps it. */
export type PostStatus = "draft" | "scheduled" | "published" | "archived"

export interface PostMeta {
  title: string
  slug: string
  excerpt: string
  status: PostStatus
  publishAt: string
  /** The language it is written in. A post is in one. */
  language: string
  /** Ids of the categories and tags it is filed under, together. */
  categoryIds: string[]
  /** Tags, by id, kept apart from categories only for how they are chosen. */
  tags: string[]
  /** The picture that goes with it, as the API keeps it: an id, not a URL. */
  coverId: string | null
  /** Where that picture is served from, for showing it here. */
  coverUrl: string
  seoTitle: string
  seoDescription: string
  canonical: string
  /** Which kind of thing this is, and what it carries for that kind. */
  kind: string
  fields: Record<string, unknown>
}

export function useStatusLabels(): Record<PostStatus, string> {
  const { t } = useLingui()
  return {
    draft: t`Draft`,
    scheduled: t`Scheduled`,
    published: t`Published`,
    archived: t`Archived`,
  }
}
