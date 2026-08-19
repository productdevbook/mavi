import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Ban, KeyRound, Loader2, Plus, Trash2, UserRound } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Person, Role } from "@api"
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
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
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

/** What the API asks for, said here so a form can refuse before a request does. */
const MINIMUM_PASSWORD = 12

/**
 * Who can sign in to this site and write on it.
 *
 * Nobody types anybody else's password: an account is invited and whoever it
 * belongs to chooses one from the link. What an administrator can do here is
 * decide what somebody may reach, stop them, or take the account away.
 */
export function PeoplePage() {
  const { t } = useLingui()

  const [people, setPeople] = React.useState<Person[] | null>(null)
  const [roles, setRoles] = React.useState<Role[]>([])
  const [inviting, setInviting] = React.useState(false)
  const [email, setEmail] = React.useState("")
  const [name, setName] = React.useState("")
  const [role, setRole] = React.useState("")
  const [busy, setBusy] = React.useState(false)
  const [removing, setRemoving] = React.useState<Person | null>(null)

  // Changing your own is a different act from anything done to somebody else,
  // and asks for the current one.
  const [current, setCurrent] = React.useState("")
  const [next, setNext] = React.useState("")

  const load = React.useCallback(() => {
    every("people.list")
      .then(setPeople)
      .catch((why: unknown) => {
        toast.error(said(why))
        setPeople((held) => held ?? [])
      })

    // Only an account that may read roles gets any; a narrower one simply sees
    // no role controls.
    api("roles.list")
      .then((r) => setRoles(r ?? []))
      .catch((why: unknown) => {
        toast.error(said(why))
        setRoles((held) => held ?? [])
      })
  }, [])

  React.useEffect(load, [load])

  const invite = async () => {
    setBusy(true)

    try {
      await api("people.invite", {
        body: { email, name: name.trim() || email, role },
      })
      setInviting(false)
      setEmail("")
      setName("")
      setRole("")
      load()
      toast.success(t`Invited. They choose their own password from the link.`)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  const changeRole = async (person: Person, roleId: string) => {
    try {
      await api("people.move", {
        path: { id: person.id },
        body: { role: roleId },
      })
      toast.success(t`Role changed`)
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const suspend = async (person: Person) => {
    try {
      await api("people.move", {
        path: { id: person.id },
        body: { role: person.role },
      })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const remove = async () => {
    if (!removing) return

    try {
      await api("people.remove", { path: { id: removing.id } })
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setRemoving(null)
    }
  }

  const changeMine = async () => {
    setBusy(true)

    try {
      await api("passwords.choose", {
        body: { token: current, password: next },
      })
      setCurrent("")
      setNext("")
      // Every session went, including this one.
      toast.success(t`Password changed — sign in again`)
      window.location.href = "/login"
    } catch (why) {
      toast.error(said(why))
      setBusy(false)
    }
  }

  return (
    <>
      <DashboardPageHeader
        className="mb-6"
        title={t`People`}
        description={t`Who can sign in to this site and write on it.`}
        actions={
          <Button onClick={() => setInviting(true)}>
            <Plus /> {t`Add someone`}
          </Button>
        }
      />

      <div className="flex max-w-2xl flex-col gap-8">
        {!people ? (
          <div className="flex justify-center py-16">
            <Loader2 className="size-6 animate-spin text-muted-foreground" />
          </div>
        ) : (
          <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
            {people.map((person) => (
              <div
                key={person.id}
                className="flex flex-wrap items-center gap-3 px-4 py-3"
              >
                <UserRound className="size-4 shrink-0 text-muted-foreground" />

                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">
                    {person.name}
                    {person.standing === "invited" && (
                      <Badge variant="secondary" className="ml-2">
                        {t`Invited`}
                      </Badge>
                    )}
                    {person.standing === "stopped" && (
                      <Badge variant="secondary" className="ml-2">
                        {t`Stopped`}
                      </Badge>
                    )}
                  </p>
                  <p className="truncate text-xs text-muted-foreground">
                    {person.email}
                  </p>
                </div>

                {roles.length > 0 && (
                  <Select
                    value={
                      roles.find(
                        (one) =>
                          one.name === person.role || one.id === person.role
                      )?.id ?? ""
                    }
                    onValueChange={(value) =>
                      void changeRole(person, value ?? "")
                    }
                  >
                    <SelectTrigger size="sm" className="w-44">
                      <SelectValue placeholder={person.role} />
                    </SelectTrigger>
                    <SelectContent>
                      {roles.map((one) => (
                        <SelectItem key={one.id} value={one.id}>
                          {one.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}

                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={
                    person.standing === "stopped" ? t`Let back in` : t`Stop`
                  }
                  onClick={() => void suspend(person)}
                >
                  <Ban />
                </Button>

                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t`Remove`}
                  onClick={() => setRemoving(person)}
                >
                  <Trash2 />
                </Button>
              </div>
            ))}
          </div>
        )}

        <section className="flex flex-col gap-3 rounded-xl border border-border p-4">
          <div className="flex items-center gap-2">
            <KeyRound className="size-4 text-muted-foreground" />
            <h2 className="text-sm font-medium">{t`Your own password`}</h2>
          </div>
          <p className="text-xs text-muted-foreground">
            {t`Changing it closes everything that is open, here and everywhere else you are signed in.`}
          </p>

          <div className="flex flex-col gap-2">
            <Label htmlFor="current">{t`The one you have`}</Label>
            <Input
              id="current"
              type="password"
              value={current}
              onChange={(event) => setCurrent(event.target.value)}
            />
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="next">{t`The new one`}</Label>
            <Input
              id="next"
              type="password"
              value={next}
              onChange={(event) => setNext(event.target.value)}
            />
          </div>

          <Button
            className="self-start"
            disabled={busy || !current || next.length < MINIMUM_PASSWORD}
            onClick={() => void changeMine()}
          >
            {busy && <Loader2 className="animate-spin" />}
            {t`Change it`}
          </Button>
        </section>
      </div>

      <AboutSomebody />

      <Dialog open={inviting} onOpenChange={setInviting}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t`Add someone`}</DialogTitle>
            <DialogDescription>
              {t`They get a link and choose their own password. Nobody here types it for them.`}
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
              <Label htmlFor="person-role">{t`What they may reach`}</Label>
              <Select
                value={role}
                onValueChange={(value) => setRole(value ?? "")}
              >
                <SelectTrigger id="person-role">
                  <SelectValue placeholder={t`Which role`} />
                </SelectTrigger>
                <SelectContent>
                  {roles.map((one) => (
                    <SelectItem key={one.id} value={one.id}>
                      {one.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setInviting(false)}
            >{t`Cancel`}</Button>
            <Button
              disabled={busy || !email.trim() || !role}
              onClick={() => void invite()}
            >
              {busy && <Loader2 className="animate-spin" />}
              {t`Invite`}
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
              {t`They stop being able to sign in. What they wrote stays, and so does what the record says they did.`}
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

/**
 * What this site holds about one person, and forgetting them.
 *
 * Somebody writes in and asks; this is the answer, and the other half of it.
 * Everything under one address across every table that holds one — an account,
 * a student, a subscriber, an order, a letter that was sent.
 *
 * What forgetting does not do is unmake an order: what a shop was paid stays,
 * with the address taken out of it.
 */
function AboutSomebody() {
  const { t } = useLingui()

  const [email, setEmail] = React.useState("")
  const [found, setFound] = React.useState<unknown>(null)
  const [busy, setBusy] = React.useState(false)

  const look = async () => {
    setBusy(true)

    try {
      const answer = await api("about.gather", {
        body: { email: email.trim() },
      })

      setFound(answer)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  const forget = async () => {
    if (
      !window.confirm(
        t`Take everything this site holds about ${email}? What was paid stays, with their address out of it. This cannot be undone.`
      )
    ) {
      return
    }

    setBusy(true)

    try {
      await api("about.forget", { body: { email: email.trim() } })
      setFound(null)
      toast.success(t`Forgotten.`)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-border p-4">
      <h2 className="text-sm font-medium">{t`What we hold about somebody`}</h2>
      <p className="text-xs text-muted-foreground">
        {t`For when somebody writes in and asks. Everything under one address, across everything this site keeps.`}
      </p>

      <div className="flex gap-2">
        <Input
          type="email"
          value={email}
          placeholder={t`somebody@example.test`}
          onChange={(event) => setEmail(event.target.value)}
        />
        <Button
          variant="outline"
          disabled={!email.trim() || busy}
          onClick={() => void look()}
        >
          {busy && <Loader2 className="animate-spin" />}
          {t`Look`}
        </Button>
        <Button
          variant="ghost"
          disabled={!email.trim() || busy}
          onClick={() => void forget()}
        >
          {t`Forget them`}
        </Button>
      </div>

      {found !== null && (
        <pre className="max-h-72 overflow-auto rounded-lg bg-muted px-3 py-2 text-xs">
          {JSON.stringify(found, null, 2)}
        </pre>
      )}
    </section>
  )
}
