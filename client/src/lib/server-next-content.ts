import type { Content, PublicationStatus } from "@api-next"

/** Read the state of a content item without leaking the wire union into UI code. */
export function contentStatus(
  content: Pick<Content, "publication">
): PublicationStatus {
  const publication = content.publication

  if (publication === "draft" || publication === "archived") {
    return publication
  }

  if (publication && typeof publication === "object") {
    if ("published" in publication) return "published"
    if ("scheduled" in publication) return "scheduled"
  }

  return "draft"
}

/** The timestamp that is meaningful to a listing row. */
export function contentPublicationDate(
  content: Pick<Content, "publication" | "updated_at" | "created_at">
): string {
  const publication = content.publication

  if (publication && typeof publication === "object") {
    const state =
      "published" in publication
        ? publication.published
        : "scheduled" in publication
          ? publication.scheduled
          : null

    if (
      state &&
      typeof state === "object" &&
      "at" in state &&
      typeof state.at === "string"
    ) {
      return state.at
    }
  }

  return content.updated_at || content.created_at
}

/** Return a publication timestamp only for states that actually have one. */
export function contentPublishAt(
  content: Pick<Content, "publication">
): string | null {
  const publication = content.publication

  if (publication && typeof publication === "object") {
    const state =
      "published" in publication
        ? publication.published
        : "scheduled" in publication
          ? publication.scheduled
          : null

    if (
      state &&
      typeof state === "object" &&
      "at" in state &&
      typeof state.at === "string"
    ) {
      return state.at
    }
  }

  return null
}
