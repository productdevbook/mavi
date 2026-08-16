/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
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

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
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

export const Route = createFileRoute("/dashboard/roles")({
  component: RolesRoute,
})

type Access = "view" | "write" | "delete"

/**
 * A role, as the API keeps it: what it may reach is a list of `area:access`
 * rather than a shape per area, so this screen is the only place that turns
 * those strings into ticks and back.
 */
type Role = {
  id: string
  key: string
  name: string
  grants: string[]
  built_in: boolean
}

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

const holds = (role: Role, capability: Capability, access: Access) =>
  role.grants.includes(`${capability}:${access}`)

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
]

const CAPABILITY_ICONS: Record<Capability, React.ComponentType<{ className?: string }>> = {
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
function RolesRoute() {
  const { t } = useLingui()
  const [roles, setRoles] = React.useState<Role[] | null>(null)
  const [creating, setCreating] = React.useState(false)
  const [name, setName] = React.useState("")
  const [label, setLabel] = React.useState("")
  const [busy, setBusy] = React.useState(false)
  const [pending, setPending] = React.useState<string | null>(null)

  const load = React.useCallback(() => {
    api("GET /api/roles")
      .then(setRoles)
      .catch((why: unknown) => {
        toast.error(said(why))
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
    next: boolean,
  ) => {
    const wanted = new Set(role.grants)

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

    const grants = [...wanted]

    setPending(`${role.key}:${capability}:${access}`)

    // Optimistic: reflect the tick at once, reconcile on the reload.
    setRoles(
      (held) =>
        held?.map((one) => (one.id === role.id ? { ...one, grants } : one)) ??
        held,
    )

    try {
      await api("PATCH /api/roles/{id}", {
        path: { id: role.id },
        body: { grants },
      })
      load()
    } catch (why) {
      toast.error(said(why))
      load()
    } finally {
      setPending(null)
    }
  }

  const create = async () => {
    setBusy(true)

    try {
      await api("POST /api/roles", {
        body: { key: name, name: label || name, grants: [] },
      })
      toast.success(t`Role made`)
      setCreating(false)
      setName("")
      setLabel("")
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  const remove = async (role: Role) => {
    if (
      !window.confirm(
        t`Delete the "${role.name}" role? Move its people to another role first — this only removes the role, not the accounts.`,
      )
    ) {
      return
    }

    try {
      await api("DELETE /api/roles/{id}", { path: { id: role.id } })
      toast.success(t`Gone`)
      load()
    } catch (why) {
      toast.error(said(why))
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
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="space-y-1">
          <h1 className="text-xl font-semibold tracking-tight">{t`Roles`}</h1>
          <p className="max-w-xl text-sm text-muted-foreground">
            {t`What each role on this site may read, write and delete. Menus follow this — a screen a role cannot open is not shown to it.`}
          </p>
        </div>
        <Button onClick={() => setCreating(true)}>
          <Plus className="size-4" /> {t`New role`}
        </Button>
      </div>

      <div className="flex flex-col gap-5">
        {roles.map((role) => {
          const locked = role.built_in
          const summary = CAPABILITY_ORDER.filter((capability) =>
            holds(role, capability, "view"),
          ).length

          return (
            <Card key={role.id} className="overflow-hidden">
              <CardHeader className="border-b border-border/60 bg-muted/30">
                <CardTitle className="flex items-center gap-2 text-base">
                  {role.name}
                  {role.built_in && (
                    <Badge variant="secondary" className="font-normal">
                      {t`Built-in`}
                    </Badge>
                  )}
                  {role.grants.length === 0 && (
                    <Badge variant="secondary" className="font-normal">
                      {t`Reaches nothing`}
                    </Badge>
                  )}
                </CardTitle>
                <CardDescription>
                  {t`Can open ${summary} of ${CAPABILITY_ORDER.length} areas.`}
                </CardDescription>
                {!role.built_in && (
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
                        {(["view", "write", "delete"] as const).map((access) => (
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
                        ))}
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {CAPABILITY_ORDER.map((capability) => {
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
                                const key = `${role.key}:${capability}:${access}`
                                return (
                                  <TableCell key={access} className="text-center">
                                    <Tooltip>
                                      <TooltipTrigger
                                        render={
                                          <span className="inline-flex" />
                                        }
                                      >
                                        <Checkbox
                                          checked={holds(role, capability, access)}
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
              <Label htmlFor="role-label">{t`Name people see`}</Label>
              <Input
                id="role-label"
                placeholder={t`Editor-in-chief`}
                value={label}
                onChange={(event) => {
                  setLabel(event.target.value)
                  if (!name.trim() || name === slug(label))
                    setName(slug(event.target.value))
                }}
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="role-name">{t`Short name`}</Label>
              <Input
                id="role-name"
                placeholder="editor-in-chief"
                value={name}
                onChange={(event) => setName(slug(event.target.value))}
              />
              <p className="text-xs text-muted-foreground">
                {t`Lower-case, letters, digits and dashes. This is what a person's account is filed under.`}
              </p>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreating(false)}>
              {t`Cancel`}
            </Button>
            <Button disabled={busy || !name.trim()} onClick={() => void create()}>
              {busy ? <Loader2 className="size-4 animate-spin" /> : null}
              {t`Make it`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

/** A label turned into a short name: lower-case, dashes for gaps. */
function slug(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
}
