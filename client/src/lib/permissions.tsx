/* eslint-disable react-refresh/only-export-components -- provider + hook share one file */
import * as React from "react"

import type { Grant } from "@api-next"

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
  can: (capability: Capability, access?: Access) => boolean
}

const PermissionContext = React.createContext<PermissionState | null>(null)

/**
 * What the signed-in person may do, received once with the current session and
 * shared by the authenticated shell.
 *
 * Menus and buttons ask this before they draw: a screen nobody may open is a
 * screen nobody is shown, and a delete nobody may press is not offered. The
 * API decides the same question again on every request — this is the panel
 * being honest about it, not the guard itself.
 *
 * The grants stay in the server's structured vocabulary instead of being
 * rebuilt from every role in the site. Aggregating all roles would show a
 * person permissions they do not hold.
 */
export function PermissionProvider({
  children,
  grants,
}: {
  children: React.ReactNode
  grants: Grant[]
}) {
  const value = React.useMemo<PermissionState>(
    () => ({
      ready: true,
      can: (capability, access = "view") => {
        return grants.some(
          (grant) =>
            grant.capability === capability && grant.action === access,
        )
      },
    }),
    [grants],
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
    path === "/dashboard/api" ||
    path === "/dashboard/settings" ||
    path === "/dashboard/portable" ||
    path === "/dashboard/visitors" ||
    path === "/dashboard/performance" ||
    path === "/dashboard/usage"
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
      can: () => true,
    }
  }

  return value
}
