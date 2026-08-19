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
  /** Category term ids assigned to this content. */
  categoryIds: string[]
  /** Tag term ids assigned to this content. */
  tags: string[]
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
