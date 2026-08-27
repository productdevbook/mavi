import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import {
  Archive,
  BarChart3,
  Eye,
  FileText,
  FolderTree,
  GraduationCap,
  Image as ImageIcon,
  Inbox,
  KanbanSquare,
  Loader2,
  Mails,
  Palette,
  PackageOpen,
  Pencil,
  Plug,
  Plus,
  Rocket,
  ScrollText,
  ShoppingCart,
  Trash2,
  UsersRound,
  Workflow,
} from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import { permissionForCapability } from "@/lib/permissions"
import { type PluginId, usePlugins } from "@/lib/plugins"
import type { Grant, Permission, Role } from "@api"
import { DashboardPageHeader } from "@/components/dashboard/dashboard-page"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"

type Access = "view" | "write" | "delete"

type Capability =
  | "content"
  | "taxonomy"
  | "media"
  | "forms"
  | "mail"
  | "flows"
  | "courses"
  | "shop"
  | "boards"
  | "design"
  | "publish"
  | "people"
  | "settings"
  | "audit"
  | "analytics"
  | "trash"
  | "portable"

const grantKey = (grant: Grant) => `${grant.capability}:${grant.action}`

function permissionFromGrant(grant: Grant): Permission | null {
  const capability = grant.capability as Capability
  if (!(capability in CAPABILITY_PLUGIN)) return null
  const needed = permissionForCapability(capability, grant.action as Access)
  return { ...needed, resource_type: null }
}

const permissionKey = (permission: Permission) =>
  `${permission.plugin}:${permission.action}:${permission.resource_type ?? "*"}`

/**
 * Keeps canonical permissions that have no legacy capability/action pair.
 * The role screen still edits the compact compatibility matrix, but saving a
 * row must not silently delete newer permissions such as sessions.revoke or
 * a resource-scoped grant that the matrix cannot represent.
 */
function permissionsForGrantSelection(
  role: Role,
  grants: Grant[],
): Permission[] {
  const representedByLegacy = new Set(
    role.grants
      .flatMap((grant) => {
        const permission = permissionFromGrant(grant)
        return permission ? [permissionKey(permission)] : []
      }),
  )
  const preserved = role.permissions.filter(
    (permission) => !representedByLegacy.has(permissionKey(permission)),
  )
  const selected = grants.flatMap((grant) => {
    const permission = permissionFromGrant(grant)
    return permission ? [permission] : []
  })
  return [
    ...new Map(
      [...preserved, ...selected].map((permission) => [
        permissionKey(permission),
        permission,
      ]),
    ).values(),
  ]
}

const permissionsForRole = (role: Role): Permission[] => {
  if (role.permissions.length > 0) return role.permissions
  return role.grants.flatMap((grant) => {
    const permission = permissionFromGrant(grant)
    return permission ? [permission] : []
  })
}

const holds = (role: Role, capability: Capability, access: Access) => {
  const needed = permissionForCapability(capability, access)
  return permissionsForRole(role).some(
    (permission) =>
      permission.plugin === needed.plugin &&
      permission.action === needed.action &&
      permission.resource_type === null,
  )
}

function grantFromKey(key: string): Grant {
  const [capability, action] = key.split(":")
  return { capability, action }
}

const CAPABILITY_ORDER: Capability[] = [
  "content",
  "taxonomy",
  "media",
  "forms",
  "mail",
  "flows",
  "courses",
  "shop",
  "boards",
  "design",
  "publish",
  "people",
  "settings",
  "audit",
  "analytics",
  "trash",
  "portable",
]

const CAPABILITY_PLUGIN: Record<Capability, PluginId> = {
  content: "writing",
  taxonomy: "writing",
  media: "writing",
  forms: "forms",
  mail: "messaging",
  flows: "automation",
  courses: "learning",
  shop: "commerce",
  boards: "boards",
  design: "writing",
  publish: "writing",
  people: "core",
  settings: "core",
  audit: "governance",
  analytics: "analytics",
  trash: "governance",
  portable: "governance",
}

const CAPABILITY_ICONS: Record<
  Capability,
  React.ComponentType<{ className?: string }>
> = {
  content: FileText,
  taxonomy: FolderTree,
  media: ImageIcon,
  forms: Inbox,
  mail: Mails,
  flows: Workflow,
  courses: GraduationCap,
  shop: ShoppingCart,
  boards: KanbanSquare,
  design: Palette,
  publish: Rocket,
  people: UsersRound,
  settings: Plug,
  audit: ScrollText,
  analytics: BarChart3,
  trash: Archive,
  portable: PackageOpen,
}

/**
 * A site's own roles — its Discord-style ranks, its AWS-style policies.
 *
 * One card per role, one row per area, three ticks per row: read, write,
 * delete. Ticking write or delete turns reading on with it; turning reading
 * off takes the other two away — because a role that may edit what it cannot
 * see is a contradiction, not a permission. The administrator card is shown
 * whole and locked: it does everything, always.
 */
export function RolesPage() {
  const { t } = useLingui()
  const { activePlugins } = usePlugins()
  const visibleCapabilities = React.useMemo(
    () =>
      CAPABILITY_ORDER.filter((capability) =>
        activePlugins.has(CAPABILITY_PLUGIN[capability]),
      ),
    [activePlugins],
  )
  const [roles, setRoles] = React.useState<Role[] | null>(null)
  const [creating, setCreating] = React.useState(false)
  const [name, setName] = React.useState("")
  const [busy, setBusy] = React.useState(false)
  const [pending, setPending] = React.useState<string | null>(null)

  const load = React.useCallback(() => {
    every("roles.list", { query: {} })
      .then(setRoles)
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setRoles((held) => held ?? [])
      })
  }, [])

  React.useEffect(load, [load])

  const CAPABILITY_LABELS: Record<Capability, string> = {
    content: t`Content`,
    taxonomy: t`Categories & tags`,
    media: t`Media`,
    forms: t`Forms`,
    mail: t`Mail`,
    flows: t`Flows`,
    courses: t`Teaching`,
    shop: t`Shop`,
    boards: t`Boards`,
    design: t`Design`,
    publish: t`Publish`,
    people: t`People`,
    settings: t`Settings`,
    audit: t`Record`,
    analytics: t`Analytics`,
    trash: t`Bin`,
    portable: t`Portability`,
  }

  const ACCESS_LABELS: Record<Access, string> = {
    view: t`Read`,
    write: t`Write`,
    delete: t`Delete`,
  }

  const toggle = async (
    role: Role,
    capability: Capability,
    access: Access,
    next: boolean
  ) => {
    const wanted = new Set(role.grants.map(grantKey))

    if (next) {
      wanted.add(`${capability}:${access}`)

      // A role that may change what it cannot see is a contradiction rather
      // than a permission.
      if (access !== "view") {
        wanted.add(`${capability}:view`)
      }
    } else {
      wanted.delete(`${capability}:${access}`)

      if (access === "view") {
        wanted.delete(`${capability}:write`)
        wanted.delete(`${capability}:delete`)
      }
    }

    const grants = [...wanted].map(grantFromKey)

    setPending(`${role.id}:${capability}:${access}`)

    // Optimistic: reflect the tick at once, reconcile on the reload.
    const permissions = permissionsForGrantSelection(role, grants)
    setRoles(
      (held) =>
        held?.map((one) =>
          one.id === role.id ? { ...one, grants, permissions } : one,
        ) ??
        held
    )

    try {
      await api("roles.grants.replace", {
        path: { id: role.id },
        body: { grants, permissions },
      })
      load()
    } catch (why) {
      toast.error(apiMessage(why))
      load()
    } finally {
      setPending(null)
    }
  }

  const create = async () => {
    setBusy(true)

    try {
      await api("roles.create", {
        body: { name: slug(name), grants: [], permissions: [] },
      })
      toast.success(t`Role made`)
      setCreating(false)
      setName("")
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    } finally {
      setBusy(false)
    }
  }

  const remove = async (role: Role) => {
    if (
      !window.confirm(
        t`Delete the "${role.name}" role? Move its people to another role first — this only removes the role, not the accounts.`
      )
    ) {
      return
    }

    try {
      await api("roles.delete", { path: { id: role.id } })
      toast.success(t`Gone`)
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  if (roles === null) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-6">
      <DashboardPageHeader
        title={t`Roles`}
        description={t`What each role on this site may read, write and delete. Menus follow this — a screen a role cannot open is not shown to it.`}
        actions={
          <Button onClick={() => setCreating(true)}>
            <Plus className="size-4" /> {t`New role`}
          </Button>
        }
      />

      <div className="flex flex-col gap-5">
        {roles.map((role) => {
          const locked = role.protected
          const summary = visibleCapabilities.filter((capability) =>
            holds(role, capability, "view")
          ).length

          return (
            <Card key={role.id} className="overflow-hidden">
              <CardHeader className="border-b border-border/60 bg-muted/30">
                <CardTitle className="flex items-center gap-2 text-base">
                  {role.name}
                  {role.protected && (
                    <Badge variant="secondary" className="font-normal">
                      {t`Owner`}
                    </Badge>
                  )}
                  {permissionsForRole(role).length === 0 && (
                    <Badge variant="secondary" className="font-normal">
                      {t`Reaches nothing`}
                    </Badge>
                  )}
                </CardTitle>
                <CardDescription>
                  {t`Can open ${summary} of ${visibleCapabilities.length} active areas.`}
                </CardDescription>
                {!role.protected && (
                  <CardAction>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="size-8 text-muted-foreground hover:text-destructive"
                      onClick={() => void remove(role)}
                      aria-label={t`Delete role`}
                    >
                      <Trash2 className="size-4" />
                    </Button>
                  </CardAction>
                )}
              </CardHeader>

              <CardContent className="p-0">
                {locked ? (
                  <p className="px-6 py-5 text-sm text-muted-foreground">
                    {t`This role came with the build and is not a site's to change.`}
                  </p>
                ) : (
                  <Table>
                    <TableHeader>
                      <TableRow className="hover:bg-transparent">
                        <TableHead className="w-1/2">{t`Area`}</TableHead>
                        {(["view", "write", "delete"] as const).map(
                          (access) => (
                            <TableHead key={access} className="text-center">
                              <span className="inline-flex items-center gap-1.5">
                                {access === "view" ? (
                                  <Eye className="size-3.5" />
                                ) : access === "write" ? (
                                  <Pencil className="size-3.5" />
                                ) : (
                                  <Trash2 className="size-3.5" />
                                )}
                                {ACCESS_LABELS[access]}
                              </span>
                            </TableHead>
                          )
                        )}
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {visibleCapabilities.map((capability) => {
                        const Icon = CAPABILITY_ICONS[capability]
                        return (
                          <TableRow key={capability}>
                            <TableCell className="font-medium">
                              <span className="inline-flex items-center gap-2">
                                <Icon className="size-4 text-muted-foreground" />
                                {CAPABILITY_LABELS[capability]}
                              </span>
                            </TableCell>
                            {(["view", "write", "delete"] as const).map(
                              (access) => {
                                const key = `${role.id}:${capability}:${access}`
                                return (
                                  <TableCell
                                    key={access}
                                    className="text-center"
                                  >
                                    <Tooltip>
                                      <TooltipTrigger
                                        render={
                                          <span className="inline-flex" />
                                        }
                                      >
                                        <Checkbox
                                          checked={holds(
                                            role,
                                            capability,
                                            access
                                          )}
                                          disabled={pending === key}
                                          onCheckedChange={(value) =>
                                            void toggle(
                                              role,
                                              capability,
                                              access,
                                              value === true
                                            )
                                          }
                                        />
                                      </TooltipTrigger>
                                      <TooltipContent>
                                        {access === "view"
                                          ? t`See ${CAPABILITY_LABELS[capability]}`
                                          : access === "write"
                                            ? t`Change ${CAPABILITY_LABELS[capability]}`
                                            : t`Delete in ${CAPABILITY_LABELS[capability]}`}
                                      </TooltipContent>
                                    </Tooltip>
                                  </TableCell>
                                )
                              }
                            )}
                          </TableRow>
                        )
                      })}
                    </TableBody>
                  </Table>
                )}
              </CardContent>
            </Card>
          )
        })}
      </div>

      <Dialog open={creating} onOpenChange={setCreating}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t`New role`}</DialogTitle>
            <DialogDescription>
              {t`Make a role, then tick what it may do below.`}
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-4">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="role-name">{t`Role name`}</Label>
              <Input
                id="role-name"
                placeholder="editor-in-chief"
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                {t`Use lower-case letters, digits, underscores or dashes.`}
              </p>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreating(false)}>
              {t`Cancel`}
            </Button>
            <Button
              disabled={busy || !name.trim()}
              onClick={() => void create()}
            >
              {busy ? <Loader2 className="size-4 animate-spin" /> : null}
              {t`Make it`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

/** A role identifier accepted by the canonical identity contract. */
function slug(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
}
