/**
 * The bin, against v1.
 *
 * What is here is only the calls: the screen keeps its own wording, and the
 * shapes come from the API's own description.
 */

import { api } from "./v1"
import type { Thrown } from "@api"

export type { Thrown }

/** Everything a site threw away, newest first. */
export function inTheBin(): Promise<Thrown[]> {
  return api("trash.list")
}

/**
 * Puts one back.
 *
 * A sort as well as an id, because the id alone does not say which table it
 * came out of and this machine will not guess.
 */
export function putBack(sort: string, id: string): Promise<void> {
  return api("trash.put-back", { path: { sort, id } })
}

/** Takes it away for good. */
export function forGood(sort: string, id: string): Promise<void> {
  return api("trash.for-good", { path: { sort, id } })
}
