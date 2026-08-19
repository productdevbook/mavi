import * as React from "react"

import { nextEvery } from "@/lib/server-next"
import type { Language } from "@api-next"

/**
 * The languages this site writes in. Loaded once per mount — the list is tiny
 * and changes rarely, so asking again per screen is cheaper than a cache.
 */
export function useLanguages() {
  const [languages, setLanguages] = React.useState<Language[]>([])
  const [loading, setLoading] = React.useState(true)

  React.useEffect(() => {
    nextEvery("languages.list", { query: {} })
      .then(setLanguages)
      .catch(() => setLanguages([]))
      .finally(() => setLoading(false))
  }, [])

  const defaultCode =
    languages.find((language) => language.is_default)?.tag ??
    languages[0]?.tag ??
    ""

  const label = React.useCallback(
    (code: string) =>
      languages.find((language) => language.tag === code)?.name ?? code,
    [languages]
  )

  return { languages, loading, defaultCode, label }
}
