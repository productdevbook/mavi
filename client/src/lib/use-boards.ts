import * as React from "react"

import { every } from "@/lib/v1"
import type { Board } from "@api"

/**
 * The boards this site has made, for the menu.
 *
 * Most sites have none, and a site with none should see nothing about them —
 * which is the whole reason this is a question rather than a fixed entry.
 */
export function useBoards() {
  const [boards, setBoards] = React.useState<Board[]>([])

  React.useEffect(() => {
    let alive = true

    every("GET /api/boards")
      .then((all) => alive && setBoards(all))
      .catch(() => alive && setBoards([]))

    return () => {
      alive = false
    }
  }, [])

  return boards
}
