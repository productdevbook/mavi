/* eslint-disable react-refresh/only-export-components -- provider + hook share one file */
import * as React from "react"

import type { Grant, Permission } from "@api"

import type { PluginId } from "@/lib/plugins"
import { usePlugins } from "@/lib/plugins"

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
  | "plugins"
  | "settings"
  | "publish"
  | "design"
  | "boards"
  | "audit"
  | "analytics"
  | "trash"
  | "portable"

type Access = "view" | "write" | "delete"

const CAPABILITY_PLUGIN: Record<Capability, PluginId> = {
  content: "writing",
  media: "writing",
  taxonomy: "writing",
  forms: "forms",
  mail: "messaging",
  flows: "automation",
  courses: "learning",
  shop: "commerce",
  people: "core",
  plugins: "core",
  settings: "core",
  publish: "writing",
  design: "writing",
  boards: "boards",
  audit: "governance",
  analytics: "analytics",
  trash: "governance",
  portable: "governance",
}

interface PermissionState {
  ready: boolean
  pluginReady: boolean
  can: (capability: Capability, access?: Access) => boolean
  hasPlugin: (plugin: PluginId) => boolean
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
  permissions = [],
}: {
  children: React.ReactNode
  grants: Grant[]
  permissions?: Permission[]
}) {
  const { activePlugins, ready: pluginReady } = usePlugins()
  const value = React.useMemo<PermissionState>(
    () => ({
      ready: true,
      pluginReady,
      hasPlugin: (plugin) => pluginReady && activePlugins.has(plugin),
      can: (capability, access = "view") => {
        const needed = permissionForCapability(capability, access)
        if (!pluginReady || !activePlugins.has(needed.plugin)) return false
        return (
          permissions.some(
            (permission) =>
              permission.plugin === needed.plugin &&
              permission.action === needed.action &&
              permission.resource_type === null,
          ) ||
          grants.some(
            (grant) =>
              grant.capability === capability && grant.action === access,
          )
        )
      },
    }),
    [grants, permissions, activePlugins, pluginReady],
  )

  return (
    <PermissionContext.Provider value={value}>
      {children}
    </PermissionContext.Provider>
  )
}

/** Maps the old panel vocabulary to the canonical Cedar business action. */
export function permissionForCapability(
  capability: Capability,
  access: Access,
): { plugin: PluginId; action: string } {
  const plugin = CAPABILITY_PLUGIN[capability]
  const action =
    capability === "plugins"
      ? ({ view: "plugins.list", write: "plugins.activate", delete: "plugins.deactivate" } as const)[access]
      : capability === "content"
      ? ({ view: "content.entry.list", write: "content.entry.update", delete: "content.entry.delete" } as const)[access]
      : capability === "taxonomy"
        ? ({ view: "taxonomy.term.view", write: "taxonomy.term.manage", delete: "taxonomy.term.delete" } as const)[access]
        : capability === "media"
          ? ({ view: "media.file.list", write: "media.file.manage", delete: "media.file.delete" } as const)[access]
          : capability === "design"
            ? ({ view: "design.change.view", write: "design.change.manage", delete: "design.change.delete" } as const)[access]
            : capability === "publish"
              ? ({ view: "content.entry.list", write: "content.entry.publish", delete: "content.entry.delete" } as const)[access]
              : capability === "people"
                ? ({ view: "people.list", write: "people.update", delete: "people.delete" } as const)[access]
                : capability === "settings"
                  ? ({ view: "settings.view", write: "settings.manage", delete: "settings.delete" } as const)[access]
                  : capability === "audit"
                    ? ({ view: "audit.view", write: "audit.manage", delete: "audit.manage" } as const)[access]
                    : capability === "trash"
                      ? ({ view: "trash.view", write: "trash.manage", delete: "trash.delete" } as const)[access]
                      : capability === "portable"
                        ? ({ view: "portable.export", write: "portable.import", delete: "portable.delete" } as const)[access]
                        : capability === "analytics"
                          ? ({ view: "view", write: "manage", delete: "manage" } as const)[access]
                          : capability === "forms"
                            ? ({ view: "submission.view", write: "form.manage", delete: "form.delete" } as const)[access]
                            : capability === "mail"
                              ? ({ view: "delivery.view", write: "delivery.manage", delete: "delivery.delete" } as const)[access]
                              : capability === "flows"
                                ? ({ view: "flow.view", write: "flow.manage", delete: "flow.delete" } as const)[access]
                                : capability === "courses"
                                  ? ({ view: "courses.course.view", write: "courses.course.manage", delete: "courses.course.delete" } as const)[access]
                                  : capability === "shop"
                                    ? ({ view: "shop.product.view", write: "shop.product.manage", delete: "shop.product.delete" } as const)[access]
                                    : capability === "boards"
                                      ? ({ view: "board.view", write: "board.manage", delete: "board.delete" } as const)[access]
                                      : "settings.view"
  return { plugin, action }
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
  if (path === "/dashboard/plugins") return "plugins"
  if (path.startsWith("/dashboard/content/")) return "content"
  if (path.startsWith("/editor")) return "content"
  if (path === "/dashboard/trash") return "trash"
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
    path === "/dashboard/languages" ||
    path === "/dashboard/api" ||
    path === "/dashboard/settings"
  )
    return "settings"
  if (path === "/dashboard/content-types") return "content"
  if (path === "/dashboard/portable") return "portable"
  if (
    path === "/dashboard/visitors" ||
    path === "/dashboard/performance" ||
    path === "/dashboard/usage"
  )
    return "analytics"
  if (path === "/dashboard/audit") return "audit"
  if (path === "/dashboard/publish") return "publish"
  if (path === "/dashboard/design") return "design"
  if (path.startsWith("/dashboard/boards")) return "boards"

  return null
}

/** The compiled feature package that owns a dashboard address. */
export function pluginOf(path: string): PluginId {
  if (
    path.startsWith("/dashboard/content") ||
    path.startsWith("/editor") ||
    path === "/dashboard/media" ||
    path === "/dashboard/categories" ||
    path === "/dashboard/tags" ||
    path === "/dashboard/design" ||
    path === "/dashboard/publish"
  )
    return "writing"
  if (path.startsWith("/dashboard/forms")) return "forms"
  if (path.startsWith("/dashboard/mail") || path === "/dashboard/letters")
    return "messaging"
  if (path === "/dashboard/flows") return "automation"
  if (
    path === "/dashboard/products" ||
    path === "/dashboard/orders" ||
    path === "/dashboard/coupons"
  )
    return "commerce"
  if (
    path === "/dashboard/videos" ||
    path === "/dashboard/students" ||
    path.startsWith("/dashboard/teaching") ||
    path.startsWith("/dashboard/courses")
  )
    return "learning"
  if (path.startsWith("/dashboard/boards")) return "boards"
  if (
    path === "/dashboard/visitors" ||
    path === "/dashboard/performance" ||
    path === "/dashboard/usage"
  )
    return "analytics"
  if (
    path === "/dashboard/audit" ||
    path === "/dashboard/trash" ||
    path === "/dashboard/portable"
  )
    return "governance"
  return "core"
}

export function usePermissions(): PermissionState {
  const value = React.useContext(PermissionContext)

  if (!value) {
    // Outside a provider (a stray render) — fail closed. The API remains the
    // final authority, but a missing provider must not make a product screen
    // appear available while its plugin/permission snapshot is unknown.
    return {
      ready: false,
      pluginReady: false,
      can: () => false,
      hasPlugin: () => false,
    }
  }

  return value
}
