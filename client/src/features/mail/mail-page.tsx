import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plus, Send, Users } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { MailList, MailReader as Subscriber } from "@api"
import {
  DashboardEmpty,
  DashboardError,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
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

function slugged(value: string): string {
  return value
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
}

export function MailPage() {
  const { t } = useLingui()

  const [lists, setLists] = React.useState<MailList[] | null>(null)
  const [error, setError] = React.useState(false)
  const [people, setPeople] = React.useState<Record<string, Subscriber[]>>({})
  const [openList, setOpenList] = React.useState<string | null>(null)

  const [makingList, setMakingList] = React.useState(false)
  const [listName, setListName] = React.useState("")

  const [writing, setWriting] = React.useState(false)
  const [listId, setListId] = React.useState("")
  const [subject, setSubject] = React.useState("")
  const [body, setBody] = React.useState("")

  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    setError(false)
    every("mail.lists.list", { query: {} })
      .then(setLists)
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setError(true)
        setLists([])
      })
  }, [])

  React.useEffect(load, [load])

  const look = async (id: string, refresh = false) => {
    setOpenList(openList === id ? null : id)

    if (people[id] && !refresh) return

    try {
      const found = await every("mail.readers.list", {
        path: { id },
        query: {},
      })

      setPeople((held) => ({ ...held, [id]: found }))
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  const [joining, setJoining] = React.useState<string | null>(null)
  const [joinEmail, setJoinEmail] = React.useState("")

  const join = async (targetListId: string) => {
    setBusy(true)

    try {
      await api("mail.readers.add", {
        path: { id: targetListId },
        body: { email: joinEmail.trim() },
      })

      setJoinEmail("")
      setJoining(null)
      setPeople((held) => {
        const rest = { ...held }
        delete rest[targetListId]
        return rest
      })
      void look(targetListId, true)
    } catch (why) {
      toast.error(apiMessage(why))
    } finally {
      setBusy(false)
    }
  }

  const makeList = async () => {
    setBusy(true)

    try {
      await api("mail.lists.create", {
        body: { name: listName.trim(), slug: slugged(listName) },
      })
      setMakingList(false)
      setListName("")
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    } finally {
      setBusy(false)
    }
  }

  const write = async () => {
    setBusy(true)

    try {
      const template = await api("mail.templates.create", {
        body: {
          key: `campaign-${Date.now()}`,
          language: "und",
          subject: subject.trim(),
          body,
        },
      })
      await api("mail.deliveries.campaign", {
        path: { id: listId },
        body: { template_id: template.id },
      })
      setWriting(false)
      setSubject("")
      setBody("")
      toast.success(t`Sent to the list.`)
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="flex max-w-3xl flex-col gap-5">
      <DashboardPageHeader
        title={t`Mail`}
        description={t`The lists this site writes to, and messages sent to them.`}
        actions={
          <div className="flex flex-wrap gap-2">
            <Button
              variant="outline"
              disabled={(lists ?? []).length === 0}
              onClick={() => {
                setListId(lists?.[0]?.id ?? "")
                setWriting(true)
              }}
            >
              <Send /> {t`Send message`}
            </Button>
            <Button onClick={() => setMakingList(true)}>
              <Plus /> {t`New list`}
            </Button>
          </div>
        }
      />

      {lists === null ? (
        <DashboardLoading />
      ) : error ? (
        <DashboardError
          message={t`The mailing lists could not be read just now.`}
        />
      ) : lists.length === 0 ? (
        <DashboardEmpty
          icon={Users}
          title={t`No lists yet.`}
          description={t`Create a list before sending a message to subscribers.`}
        />
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {lists.map((list) => (
            <div key={list.id} className="px-4 py-3">
              <button
                type="button"
                className="flex w-full items-center gap-3 text-left"
                onClick={() => void look(list.id)}
              >
                <Users className="size-4 shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate text-sm font-medium">
                  {list.name}
                </span>
                <span className="text-xs text-muted-foreground">
                  {people[list.id]
                    ? `${people[list.id].length} on it`
                    : `${list.subscriber_count} reading`}
                </span>
              </button>

              {openList === list.id && (
                <div className="mt-2 flex flex-col gap-1">
                  {joining === list.id ? (
                    <form
                      className="flex gap-2"
                      onSubmit={(event) => {
                        event.preventDefault()
                        void join(list.id)
                      }}
                    >
                      <Input
                        type="email"
                        value={joinEmail}
                        placeholder={t`somebody@example.test`}
                        onChange={(event) => setJoinEmail(event.target.value)}
                      />
                      <Button
                        type="submit"
                        size="sm"
                        disabled={busy || !joinEmail.trim()}
                      >
                        {t`Add`}
                      </Button>
                    </form>
                  ) : (
                    <Button
                      variant="ghost"
                      size="sm"
                      className="self-start"
                      onClick={() => setJoining(list.id)}
                    >
                      <Plus /> {t`Add somebody`}
                    </Button>
                  )}

                  {(people[list.id] ?? []).length === 0 ? (
                    <p className="text-xs text-muted-foreground">
                      {t`Nobody on it yet.`}
                    </p>
                  ) : (
                    (people[list.id] ?? []).map((one) => (
                      <p
                        key={one.id}
                        className="truncate text-xs text-muted-foreground"
                      >
                        {one.email}
                      </p>
                    ))
                  )}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      <Dialog open={makingList} onOpenChange={setMakingList}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t`A new list`}</DialogTitle>
            <DialogDescription>
              {t`People join it from your own pages, and leave it from any letter this site sends.`}
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-2">
            <Label htmlFor="list-name">{t`What it is called`}</Label>
            <Input
              id="list-name"
              value={listName}
              onChange={(event) => setListName(event.target.value)}
            />
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setMakingList(false)}>
              {t`Cancel`}
            </Button>
            <Button
              disabled={!listName.trim() || busy}
              onClick={() => void makeList()}
            >
              {busy && <Loader2 className="animate-spin" />}
              {t`Make it`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={writing} onOpenChange={setWriting}>
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t`Send message`}</DialogTitle>
            <DialogDescription>
              {t`Written now, sent to everyone on the list.`}
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-2">
              <Label htmlFor="campaign-list">{t`To which list`}</Label>
              <Select
                value={listId}
                onValueChange={(value) => setListId(value ?? "")}
              >
                <SelectTrigger id="campaign-list">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {(lists ?? []).map((list) => (
                    <SelectItem key={list.id} value={list.id}>
                      {list.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="campaign-subject">{t`Subject`}</Label>
              <Input
                id="campaign-subject"
                value={subject}
                onChange={(event) => setSubject(event.target.value)}
              />
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="campaign-body">{t`What it says`}</Label>
              <Textarea
                id="campaign-body"
                rows={10}
                value={body}
                onChange={(event) => setBody(event.target.value)}
              />
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setWriting(false)}>
              {t`Cancel`}
            </Button>
            <Button
              disabled={!subject.trim() || !listId || busy}
              onClick={() => void write()}
            >
              {busy && <Loader2 className="animate-spin" />}
              {t`Send`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
