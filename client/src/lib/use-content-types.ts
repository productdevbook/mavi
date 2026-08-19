import * as React from "react"
import { useLingui } from "@lingui/react/macro"

import { api } from "@/lib/v1"
import type { AKind } from "@api"

export type ContentType = AKind & { key: string; plural?: string }

// Shared by every list of kinds on screen at once. Adding one is done on the
// content-types page and read by the sidebar, which is a different mount: kept
// local, a new kind appeared where it was made and nowhere else until the page
// was reloaded.
let asOfAll = 0
const listeners = new Set<() => void>()

function announce() {
  asOfAll += 1

  for (const tell of listeners) {
    tell()
  }
}

function subscribe(tell: () => void) {
  listeners.add(tell)

  return () => {
    listeners.delete(tell)
  }
}

/**
 * What this site publishes. Loaded once per mount, like the languages: the
 * list is small, changes rarely, and asking again costs less than a cache that
 * has to be told when it is wrong.
 */
export function useContentTypes() {
  const { t } = useLingui()
  const [loaded, setLoaded] = React.useState<AKind[]>([])
  const [loading, setLoading] = React.useState(true)

  // Bumped rather than calling the fetch again, so that reloading is a change
  // of state the effect reacts to rather than a second thing that sets it.
  const asOf = React.useSyncExternalStore(
    subscribe,
    () => asOfAll,
    () => asOfAll,
  )

  React.useEffect(() => {
    let cancelled = false

    api("kinds.list")
      .then((all) => {
        if (!cancelled) setLoaded(all)
      })
      .catch(() => {
        if (!cancelled) setLoaded([])
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [asOf])

  const types: ContentType[] = React.useMemo(() => {
    const ours: Record<string, { was: string; name: string; plural: string }> = {
      post: { was: "Post", name: t`Post`, plural: t`Posts` },
      page: { was: "Page", name: t`Page`, plural: t`Pages` },
    }

    return loaded.map((kind) => {
      const seeded = ours[kind.kind]

      return seeded && kind.name === seeded.was
        ? { ...kind, key: kind.kind, name: seeded.name, plural: seeded.plural }
        : { ...kind, key: kind.kind, plural: kind.name }
    })
  }, [loaded, t])

  const find = React.useCallback(
    (key: string) => types.find((kind) => kind.kind === key || kind.key === key),
    [types],
  )

  const reload = React.useCallback(() => announce(), [])

  return { types, loading, find, reload }
}
