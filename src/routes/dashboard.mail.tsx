/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { Link, createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Mails, Plus, Send, Users } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Campaign, MailList, Subscriber } from "../../server/types/mavicms"
import { Badge } from "@/components/ui/badge"
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"

export const Route = createFileRoute("/dashboard/mail")({
  component: MailRoute,
})

/**
 * Who a site writes to, and what it has written.
 *
 * Two things rather than one screen with a mode: a list is people, a campaign
 * is one letter to one list. What a site sends to one person — an invitation,
 * a receipt — is under Letters, because that is written once and sent by the
 * machine rather than by somebody pressing send.
 */
function MailRoute() {
  const { t } = useLingui()

  const [lists, setLists] = React.useState<MailList[] | null>(null)
  const [campaigns, setCampaigns] = React.useState<Campaign[] | null>(null)
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
    every("GET /api/mail/lists")
      .then(setLists)
      .catch((why: unknown) => {
        toast.error(said(why))
        setLists([])
      })

    every("GET /api/mail/campaigns")
      .then(setCampaigns)
      .catch((why: unknown) => {
        toast.error(said(why))
        setCampaigns([])
      })
  }, [])

  React.useEffect(load, [load])

  const look = async (id: string) => {
    setOpenList(openList === id ? null : id)

    if (people[id]) return

    try {
      const found = await every("GET /api/mail/lists/{id}/subscribers", {
        path: { id },
      })

      setPeople((held) => ({ ...held, [id]: found }))
    } catch (why) {
      toast.error(said(why))
    }
  }

  const [joining, setJoining] = React.useState<string | null>(null)
  const [joinEmail, setJoinEmail] = React.useState("")

  const join = async (listId: string) => {
    setBusy(true)

    try {
      await api("POST /api/mail/lists/{id}/subscribers", {
        path: { id: listId },
        body: { email: joinEmail.trim() },
      })

      setJoinEmail("")
      setJoining(null)
      setPeople((held) => {
        const rest = { ...held }

        delete rest[listId]

        return rest
      })
      void look(listId)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  const makeList = async () => {
    setBusy(true)

    try {
      await api("POST /api/mail/lists", { body: { name: listName.trim() } })
      setMakingList(false)
      setListName("")
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  const write = async () => {
    setBusy(true)

    try {
      await api("POST /api/mail/campaigns", {
        body: { list_id: listId, subject: subject.trim(), body },
      })
      setWriting(false)
      setSubject("")
      setBody("")
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  const send = async (campaign: Campaign) => {
    try {
      await api("POST /api/mail/campaigns/{id}/send", {
        path: { id: campaign.id },
      })
      toast.success(t`On its way. Sending happens in the background.`)
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const states: Record<string, string> = {
    draft: t`Not sent`,
    sending: t`Sending`,
    sent: t`Sent`,
  }

  return (
    <>
      <div className="mb-6">
        <h1 className="text-lg font-semibold">{t`Mail`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`The lists this site writes to, and what it has written to them.`}
        </p>
      </div>

      <Tabs defaultValue="campaigns">
        <TabsList>
          <TabsTrigger value="campaigns">{t`Campaigns`}</TabsTrigger>
          <TabsTrigger value="lists">{t`Lists`}</TabsTrigger>
        </TabsList>

        <TabsContent value="campaigns" className="flex flex-col gap-4 pt-4">
          <Button
            className="self-start"
            disabled={(lists ?? []).length === 0}
            onClick={() => {
              setListId(lists?.[0]?.id ?? "")
              setWriting(true)
            }}
          >
            <Plus /> {t`Write one`}
          </Button>

          {campaigns === null ? (
            <div className="flex justify-center py-16">
              <Loader2 className="size-6 animate-spin text-muted-foreground" />
            </div>
          ) : campaigns.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border py-16 text-center">
              <Mails className="mx-auto mb-3 size-8 text-muted-foreground" />
              <p className="text-sm text-muted-foreground">
                {t`Nothing written yet.`}
              </p>
            </div>
          ) : (
            <div className="flex max-w-3xl flex-col divide-y divide-border rounded-xl border border-border">
              {campaigns.map((campaign) => (
                <div
                  key={campaign.id}
                  className="flex flex-wrap items-center gap-3 px-4 py-3"
                >
                  <Link
                    to="/dashboard/mail/campaigns/$campaignId"
                    params={{ campaignId: campaign.id }}
                    className="min-w-0 flex-1"
                  >
                    <p className="truncate text-sm font-medium hover:underline">
                      {campaign.subject}
                    </p>
                    <p className="truncate text-xs text-muted-foreground">
                      {new Date(campaign.created_at).toLocaleString()}
                      {campaign.sent_count > 0
                        ? ` · ${t`${campaign.sent_count} sent`}`
                        : ""}
                    </p>
                  </Link>

                  <Badge
                    variant={campaign.state === "sent" ? "default" : "secondary"}
                  >
                    {states[campaign.state] ?? campaign.state}
                  </Badge>

                  {campaign.state === "draft" && (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => void send(campaign)}
                    >
                      <Send /> {t`Send it`}
                    </Button>
                  )}
                </div>
              ))}
            </div>
          )}
        </TabsContent>

        <TabsContent value="lists" className="flex flex-col gap-4 pt-4">
          <Button className="self-start" onClick={() => setMakingList(true)}>
            <Plus /> {t`New list`}
          </Button>

          {lists === null ? (
            <div className="flex justify-center py-16">
              <Loader2 className="size-6 animate-spin text-muted-foreground" />
            </div>
          ) : lists.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border py-16 text-center">
              <Users className="mx-auto mb-3 size-8 text-muted-foreground" />
              <p className="text-sm text-muted-foreground">{t`No lists yet.`}</p>
            </div>
          ) : (
            <div className="flex max-w-3xl flex-col divide-y divide-border rounded-xl border border-border">
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
                    {people[list.id] && (
                      <span className="text-xs text-muted-foreground">
                        {t`${people[list.id].length} on it`}
                      </span>
                    )}
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
                          <Button type="submit" size="sm" disabled={busy}>
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
                            {one.state !== "subscribed" ? ` · ${one.state}` : ""}
                          </p>
                        ))
                      )}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </TabsContent>
      </Tabs>

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
            <DialogTitle>{t`A campaign`}</DialogTitle>
            <DialogDescription>
              {t`Written now, sent when you press send. Everybody on the list gets it once.`}
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
              {t`Save it`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
