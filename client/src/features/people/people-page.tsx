import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Ban, Loader2, Plus, Trash2, UserRound } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { PersonRecord, Role } from "@api"
import { DashboardPageHeader } from "@/components/dashboard/dashboard-page"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

const MINIMUM_PASSWORD = 12

/** Accounts and their site roles. Every mutation maps to one canonical operation. */
export function PeoplePage() {
  const { t } = useLingui()
  const [people, setPeople] = React.useState<PersonRecord[] | null>(null)
  const [roles, setRoles] = React.useState<Role[]>([])
  const [adding, setAdding] = React.useState(false)
  const [email, setEmail] = React.useState("")
  const [name, setName] = React.useState("")
  const [password, setPassword] = React.useState("")
  const [roleId, setRoleId] = React.useState("")
  const [busy, setBusy] = React.useState(false)
  const [removing, setRemoving] = React.useState<PersonRecord | null>(null)

  const loadPeople = React.useCallback(() => {
    every("people.list", { query: {} })
      .then(setPeople)
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setPeople((held) => held ?? [])
      })
  }, [])

  const loadRoles = React.useCallback(() => {
    every("roles.list", { query: {} })
      .then(setRoles)
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
      })
  }, [])

  const load = React.useCallback(() => {
    loadPeople()
    loadRoles()
  }, [loadPeople, loadRoles])

  React.useEffect(load, [load])

  const resetForm = () => {
    setEmail("")
    setName("")
    setPassword("")
    setRoleId("")
  }

  const create = async () => {
    setBusy(true)

    try {
      await api("people.create", {
        body: {
          email: email.trim(),
          name: name.trim() || email.trim(),
          password,
          role_ids: roleId ? [roleId] : [],
        },
      })
      setAdding(false)
      resetForm()
      load()
      toast.success(t`Account created`)
    } catch (why) {
      toast.error(apiMessage(why))
    } finally {
      setBusy(false)
    }
  }

  const replaceRole = async (person: PersonRecord, nextRoleId: string) => {
    if (
      !nextRoleId ||
      (person.role_ids.length === 1 && person.role_ids[0] === nextRoleId)
    ) {
      return
    }

    try {
      await api("people.roles.replace", {
        path: { id: person.id },
        body: { role_ids: [nextRoleId] },
      })
      loadPeople()
      toast.success(t`Role changed`)
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  const toggleSuspension = async (person: PersonRecord) => {
    if (person.status === "removed") return

    try {
      await api("people.status.update", {
        path: { id: person.id },
        body: {
          status: person.status === "suspended" ? "active" : "suspended",
        },
      })
      loadPeople()
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  const remove = async () => {
    if (!removing) return

    try {
      await api("people.status.update", {
        path: { id: removing.id },
        body: { status: "removed" },
      })
      loadPeople()
      toast.success(t`Account removed`)
    } catch (why) {
      toast.error(apiMessage(why))
    } finally {
      setRemoving(null)
    }
  }

  return (
    <>
      <DashboardPageHeader
        className="mb-6"
        title={t`People`}
        description={t`Accounts that can sign in to this site and the roles that shape their access.`}
        actions={
          <Button onClick={() => setAdding(true)}>
            <Plus className="size-4" /> {t`Add someone`}
          </Button>
        }
      />

      <div className="flex max-w-3xl flex-col gap-8">
        {!people ? (
          <div className="flex justify-center py-16">
            <Loader2 className="size-6 animate-spin text-muted-foreground" />
          </div>
        ) : people.length === 0 ? (
          <div className="rounded-xl border border-dashed border-border px-4 py-10 text-center text-sm text-muted-foreground">
            {t`No accounts yet.`}
          </div>
        ) : (
          <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
            {people.map((person) => {
              const selectedRole = person.role_ids[0] ?? ""
              const roleNames = person.role_ids
                .map((id) => roles.find((role) => role.id === id)?.name)
                .filter(Boolean)
                .join(", ")

              return (
                <div
                  key={person.id}
                  className="flex flex-wrap items-center gap-3 px-4 py-3"
                >
                  <UserRound className="size-4 shrink-0 text-muted-foreground" />

                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium">
                      {person.name}
                      {person.status !== "active" && (
                        <Badge variant="secondary" className="ml-2">
                          {person.status === "suspended"
                            ? t`Suspended`
                            : t`Removed`}
                        </Badge>
                      )}
                    </p>
                    <p className="truncate text-xs text-muted-foreground">
                      {person.email}
                      {roleNames ? ` · ${roleNames}` : ""}
                    </p>
                  </div>

                  {roles.length > 0 && person.status !== "removed" && (
                    <Select
                      value={selectedRole}
                      onValueChange={(value) =>
                        void replaceRole(person, value ?? "")
                      }
                    >
                      <SelectTrigger size="sm" className="w-44">
                        <SelectValue placeholder={t`No role`} />
                      </SelectTrigger>
                      <SelectContent>
                        {roles.map((role) => (
                          <SelectItem key={role.id} value={role.id}>
                            {role.name}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  )}

                  {person.status !== "removed" && (
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={
                        person.status === "suspended"
                          ? t`Let back in`
                          : t`Suspend`
                      }
                      onClick={() => void toggleSuspension(person)}
                    >
                      <Ban className="size-4" />
                    </Button>
                  )}

                  {person.status !== "removed" && (
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t`Remove`}
                      onClick={() => setRemoving(person)}
                    >
                      <Trash2 className="size-4" />
                    </Button>
                  )}
                </div>
              )
            })}
          </div>
        )}
      </div>

      <Dialog open={adding} onOpenChange={setAdding}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t`Add someone`}</DialogTitle>
            <DialogDescription>
              {t`Create the account with an initial password, then share that password securely with its owner.`}
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-2">
              <Label htmlFor="person-email">{t`Email`}</Label>
              <Input
                id="person-email"
                type="email"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
              />
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="person-name">{t`Name`}</Label>
              <Input
                id="person-name"
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="person-password">{t`Initial password`}</Label>
              <Input
                id="person-password"
                type="password"
                minLength={MINIMUM_PASSWORD}
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                {t`Use at least ${MINIMUM_PASSWORD} characters. The owner can use the password reset flow later.`}
              </p>
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="person-role">{t`Role`}</Label>
              <Select
                value={roleId}
                onValueChange={(value) => setRoleId(value ?? "")}
              >
                <SelectTrigger id="person-role">
                  <SelectValue placeholder={t`Choose a role`} />
                </SelectTrigger>
                <SelectContent>
                  {roles.map((role) => (
                    <SelectItem key={role.id} value={role.id}>
                      {role.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setAdding(false)}>
              {t`Cancel`}
            </Button>
            <Button
              disabled={
                busy ||
                !email.trim() ||
                password.length < MINIMUM_PASSWORD ||
                (roles.length > 0 && !roleId)
              }
              onClick={() => void create()}
            >
              {busy && <Loader2 className="size-4 animate-spin" />}
              {t`Create account`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={removing !== null}
        onOpenChange={(open) => !open && setRemoving(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Remove this account?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`The account will no longer be able to sign in. Its content and audit history remain.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void remove()}>
              {t`Remove`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
