import * as React from "react"

import { every } from "@/lib/api"
import { usePlugins } from "@/lib/plugins"
import type { Board } from "@api"

/**
 * The boards this site has made, for the menu.
 *
 * Most sites have none, and a site with none should see nothing about them —
 * which is the whole reason this is a question rather than a fixed entry.
 */
export function useBoards() {
  const [boards, setBoards] = React.useState<Board[]>([])
  const { activePlugins, ready: pluginsReady } = usePlugins()

  React.useEffect(() => {
    if (!pluginsReady || !activePlugins.has("boards")) {
      setBoards([])
      return undefined
    }

    let alive = true

    every("boards.list", { query: {} })
      .then((all) => alive && setBoards(all))
      .catch(() => alive && setBoards([]))

    return () => {
      alive = false
    }
  }, [activePlugins, pluginsReady])

  return boards
}
