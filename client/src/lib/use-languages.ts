import * as React from "react"

import { api } from "@/lib/v1"
import type { Language } from "@api"

/**
 * The languages this site writes in. Loaded once per mount — the list is tiny
 * and changes rarely, so asking again per screen is cheaper than a cache.
 */
export function useLanguages() {
  const [languages, setLanguages] = React.useState<Language[]>([])
  const [loading, setLoading] = React.useState(true)

  React.useEffect(() => {
    api("GET /api/languages")
      .then(setLanguages)
      .catch(() => setLanguages([]))
      .finally(() => setLoading(false))
  }, [])

  const defaultCode =
    languages.find((language) => language.is_default)?.code ??
    languages[0]?.code ??
    ""

  const label = React.useCallback(
    (code: string) =>
      languages.find((language) => language.code === code)?.name ?? code,
    [languages],
  )

  return { languages, loading, defaultCode, label }
}
