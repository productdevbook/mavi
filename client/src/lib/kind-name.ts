export interface Called {
  name: string
  plural?: string
}

export interface KindWithNames {
  name: string
  plural?: string
  names?: Record<string, Called>
}

export function calledIn(
  kind: KindWithNames,
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
