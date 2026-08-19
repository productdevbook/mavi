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
    api("languages.list")
      .then(setLanguages)
      .catch(() => setLanguages([]))
      .finally(() => setLoading(false))
  }, [])

  const defaultCode =
    languages.find((language) => language.is_the_sites_own)?.tag ??
    languages[0]?.tag ??
    ""

  const label = React.useCallback(
    (code: string) =>
      languages.find((language) => language.tag === code)?.name ?? code,
    [languages],
  )

  return { languages, loading, defaultCode, label }
}
