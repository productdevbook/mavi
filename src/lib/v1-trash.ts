/**
 * The bin, against v1.
 *
 * What is here is only the calls: the screen keeps its own wording, and the
 * shapes come from the API's own description.
 */

import { api, every } from "./v1"
import type { Gives } from "./v1"

export type Thrown = Gives<"GET /api/trash"> extends { items: (infer T)[] }
  ? T
  : never

/** Everything a site threw away, newest first. */
export function inTheBin(): Promise<Thrown[]> {
  return every("GET /api/trash")
}

/**
 * Puts one back.
 *
 * A kind as well as an id, because the id alone does not say which table it
 * came out of and this machine will not guess.
 */
export function putBack(kind: string, id: string): Promise<unknown> {
  return api("POST /api/trash/{kind}/{id}", { path: { kind, id } })
}

/** Takes it away for good. */
export function forGood(kind: string, id: string): Promise<void> {
  return api("DELETE /api/trash/{kind}/{id}", { path: { kind, id } })
}
