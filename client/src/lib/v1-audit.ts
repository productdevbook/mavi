/**
 * Reading the record.
 *
 * The API writes what happened as a phrase — "wrote", "removed a language",
 * "signed in" — rather than as a key, so there is no table of names to keep in
 * step with it. What is here instead is the part a screen cannot derive: which
 * of those phrases somebody scans a page looking for, and what to draw beside
 * each.
 */

import {
  Ban,
  KeyRound,
  LogIn,
  LogOut,
  Palette,
  ShieldAlert,
  Trash2,
  UserCog,
  UserPlus,
  Workflow,
} from "lucide-react"
import type * as React from "react"

import { api } from "./v1"
import type { Receipt, ReceiptPage } from "@api"

export type Entry = Receipt
export type { Receipt, ReceiptPage }

/**
 * What a phrase is drawn as, by the word it starts with. Read in order, so a
 * longer phrase can be answered before the word it starts with.
 */
const DRAWN: [string, React.ComponentType<{ className?: string }>, boolean][] = [
  ["signed in", LogIn, false],
  ["signed out", LogOut, false],
  ["sessions.", LogIn, false],
  ["refused", Ban, true],
  ["removed", Trash2, true],
  ["took", Trash2, true],
  ["delete", Trash2, true],
  ["invited", UserPlus, false],
  ["people.invite", UserPlus, false],
  ["changed a person", UserCog, false],
  ["chose a password", KeyRound, false],
  ["passwords.", KeyRound, false],
  ["replaced", KeyRound, true],
  ["wrote a flow", Workflow, true],
  ["flows.", Workflow, true],
  ["wrote a theme file", Palette, false],
  ["design.", Palette, false],
  ["published", LogOut, false],
  ["writings.", Palette, false],
]

export function drawing(action: string): {
  icon: React.ComponentType<{ className?: string }>
  grave: boolean
} {
  const found = DRAWN.find(([starts]) => action.startsWith(starts))

  return found
    ? { icon: found[1], grave: found[2] }
    : { icon: ShieldAlert, grave: false }
}

/** A page of the record, newest first. */
export function record(asking: {
  did?: string
  about?: string
  after?: string
  limit?: number
}): Promise<ReceiptPage> {
  return api("GET /api/audit", { query: asking })
}

/** Where the whole thing is downloaded from. */
export const RECORD_AS_A_FILE = "/api/audit"
