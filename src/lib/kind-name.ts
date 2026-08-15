import type { ContentType } from "../../server/types/mavicms"

/** What a kind is called in one language, as the API keeps it. */
interface Called {
  name?: string
  plural?: string
}

/**
 * What to call a kind, in the language the panel is being read in.
 *
 * A kind has one name and one plural, plus whatever a site has written for
 * other languages: without that, a site writing in three had every screen
 * labelled in whichever one the kind was made in — a Turkish editor reading
 * "Book", or an English one reading "Kitap".
 *
 * The language is matched loosely: a panel in `en-GB` takes an `en` name,
 * because a site that wrote one meant it for both. Nothing to match falls back
 * to what the kind is called, which is what every site with one language will
 * always use.
 */
export function calledIn(
  kind: Pick<ContentType, "name" | "plural" | "names">,
  locale: string,
  plural = false,
): string {
  const fallback = plural ? kind.plural || kind.name : kind.name
  const names = (kind.names ?? {}) as Record<string, Called>

  const found = names[locale] ?? names[locale.split("-")[0]]

  if (!found) {
    return fallback
  }

  return (plural ? found.plural || found.name : found.name) || fallback
}
