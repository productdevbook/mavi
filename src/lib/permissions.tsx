/* eslint-disable react-refresh/only-export-components -- provider + hook share one file */
import * as React from "react"

import { api } from "@/lib/v1"

export type Capability =
  | "content"
  | "media"
  | "taxonomy"
  | "forms"
  | "mail"
  | "flows"
  | "courses"
  | "shop"
  | "people"
  | "settings"
  | "publish"
  | "design"
  | "boards"
  | "audit"

type Access = "view" | "write" | "delete"

interface PermissionState {
  ready: boolean
  role: string
  can: (capability: Capability, access?: Access) => boolean
  reload: () => void
}

const PermissionContext = React.createContext<PermissionState | null>(null)

/**
 * What the signed-in person may do, asked once and shared.
 *
 * Menus and buttons ask this before they draw: a screen nobody may open is a
 * screen nobody is shown, and a delete nobody may press is not offered. The
 * API decides the same question again on every request — this is the panel
 * being honest about it, not the guard itself.
 *
 * What it reads is the list of grants the API hands back with the account:
 * `content:write`, `shop:view`. Held as strings rather than unpacked, because
 * that is what the API says and two spellings of one thing is how a panel and
 * a server come to disagree.
 */
export function PermissionProvider({
  children,
}: {
  children: React.ReactNode
}) {
  const [grants, setGrants] = React.useState<string[] | null>(null)
  const [role, setRole] = React.useState("")
  const [ready, setReady] = React.useState(false)

  const reload = React.useCallback(() => {
    api("GET /api/auth/me")
      .then((me) => {
        setGrants(me.grants)
        setRole(me.role)
        setReady(true)
      })
      .catch(() => {
        // The machine's own console and the setup screens have no grants of
        // this kind; a failure means "nothing this surface knows about", so
        // nothing is hidden by an error.
        setGrants(null)
        setReady(true)
      })
  }, [])

  React.useEffect(reload, [reload])

  const value = React.useMemo<PermissionState>(
    () => ({
      ready,
      role,
      can: (capability, access = "view") => {
        if (!grants) return true

        // Everything is `own` or not: a grant over one's own carries the same
        // word, so the panel shows the screen and the API decides the row.
        return grants.some(
          (grant) =>
            grant === `${capability}:${access}` ||
            grant === `${capability}:${access}:own`,
        )
      },
      reload,
    }),
    [grants, role, ready, reload],
  )

  return (
    <PermissionContext.Provider value={value}>
      {children}
    </PermissionContext.Provider>
  )
}

/**
 * Which capability a screen belongs to.
 *
 * One list rather than two: the menu hid what a role could not use and every
 * screen still rendered in full to anybody who typed its address, so what was
 * hidden was the door rather than the room.
 */
export function capabilityOf(path: string): Capability | null {
  if (path === "/dashboard") return null
  if (path.startsWith("/dashboard/content/")) return "content"
  if (path.startsWith("/editor")) return "content"
  if (path === "/dashboard/trash") return "content"
  if (path === "/dashboard/media") return "media"
  if (path === "/dashboard/categories" || path === "/dashboard/tags")
    return "taxonomy"
  if (path.startsWith("/dashboard/forms")) return "forms"
  if (path.startsWith("/dashboard/mail") || path === "/dashboard/letters")
    return "mail"
  if (path === "/dashboard/flows") return "flows"
  if (
    path === "/dashboard/videos" ||
    path === "/dashboard/students" ||
    path.startsWith("/dashboard/teaching") ||
    path.startsWith("/dashboard/courses")
  )
    return "courses"
  if (
    path === "/dashboard/orders" ||
    path === "/dashboard/coupons" ||
    path === "/dashboard/products"
  )
    return "shop"
  if (path === "/dashboard/users" || path === "/dashboard/roles")
    return "people"
  if (
    path === "/dashboard/content-types" ||
    path === "/dashboard/languages" ||
    path === "/dashboard/plugins" ||
    path === "/dashboard/api" ||
    path === "/dashboard/settings" ||
    path === "/dashboard/visitors" ||
    path === "/dashboard/performance"
  )
    return "settings"
  if (path === "/dashboard/audit") return "audit"
  if (path === "/dashboard/publish") return "publish"
  if (path === "/dashboard/design") return "design"
  if (path.startsWith("/dashboard/boards")) return "boards"

  return null
}

export function usePermissions(): PermissionState {
  const value = React.useContext(PermissionContext)

  if (!value) {
    // Outside a provider (a stray render) — permissive, the API still gates.
    return {
      ready: true,
      role: "",
      can: () => true,
      reload: () => {},
    }
  }

  return value
}
