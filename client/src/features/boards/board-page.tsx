import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { ArrowLeft, Loader2, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { Board, BoardList, Card as OneCard } from "@api"
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
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

export function BoardPage({
  boardId,
  onBack,
}: {
  boardId: string
  onBack: () => void
}) {
  const { t } = useLingui()

  const [board, setBoard] = React.useState<Board | null>(null)
  const [lists, setLists] = React.useState<BoardList[]>([])
  const [cards, setCards] = React.useState<OneCard[]>([])
  const [adding, setAdding] = React.useState<string | null>(null)
  const [title, setTitle] = React.useState("")
  const [detail, setDetail] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    Promise.all([
      api("boards.read", { path: { id: boardId } }),
      every("boards.lists.list", { path: { id: boardId }, query: {} }),
    ])
      .then(async ([b, nextLists]) => {
        const nextCards = (
          await Promise.all(
            nextLists.map((list) =>
              every("boards.cards.list", { path: { id: list.id }, query: {} })
            )
          )
        ).flat()
        setBoard(b)
        setLists(nextLists)
        setCards(nextCards)
      })
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setBoard(null)
      })
  }, [boardId])

  React.useEffect(load, [load])

  const add = async (listId: string) => {
    setBusy(true)

    try {
      await api("boards.cards.create", {
        path: { id: listId },
        body: {
          title: title.trim(),
          description: detail.trim() || null,
          assignee_id: null,
        },
      })
      setAdding(null)
      setTitle("")
      setDetail("")
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    } finally {
      setBusy(false)
    }
  }

  const move = async (card: OneCard, listId: string) => {
    try {
      await api("boards.cards.move", {
        path: { id: card.id },
        body: { list_id: listId, before_card_id: null },
      })
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  const remove = async (card: OneCard) => {
    try {
      await api("boards.cards.delete", { path: { id: card.id } })
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  if (!board) {
    return <DashboardLoading />
  }

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={board.name}
        description={t`Move cards through the stages of this board.`}
        actions={
          <Button variant="ghost" size="sm" onClick={onBack}>
            <ArrowLeft /> {t`Boards`}
          </Button>
        }
      />

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {lists.map((list) => {
          const stageCards = cards.filter((c) => c.list_id === list.id)

          return (
            <section
              key={list.id}
              className="flex flex-col gap-2 rounded-xl border border-border p-3"
            >
              <div className="flex items-center justify-between">
                <h2 className="text-sm font-medium">{list.name}</h2>
                <span className="text-xs text-muted-foreground">
                  {stageCards.length}
                </span>
              </div>

              {stageCards.map((card) => (
                <div
                  key={card.id}
                  className="flex flex-col gap-2 rounded-lg border border-border bg-card p-2.5"
                >
                  <div className="flex items-start gap-2">
                    <p className="min-w-0 flex-1 text-sm font-medium">
                      {card.title}
                    </p>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t`Remove`}
                      onClick={() => void remove(card)}
                    >
                      <Trash2 />
                    </Button>
                  </div>

                  {card.description && (
                    <p className="text-xs text-muted-foreground">
                      {card.description}
                    </p>
                  )}

                  <Select
                    value={card.list_id}
                    onValueChange={(value) =>
                      void move(card, value ?? card.list_id)
                    }
                  >
                    <SelectTrigger size="sm">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {lists.map((one) => (
                        <SelectItem key={one.id} value={one.id}>
                          {one.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              ))}

              <Button
                variant="ghost"
                size="sm"
                className="self-start"
                onClick={() => setAdding(list.id)}
              >
                <Plus /> {t`A card`}
              </Button>
            </section>
          )
        })}
      </div>

      <Dialog
        open={adding !== null}
        onOpenChange={(open) => !open && setAdding(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t`A new card`}</DialogTitle>
          </DialogHeader>

          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-2">
              <Label htmlFor="card-title">{t`What it is`}</Label>
              <Input
                id="card-title"
                value={title}
                onChange={(event) => setTitle(event.target.value)}
              />
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="card-detail">{t`Anything else`}</Label>
              <Textarea
                id="card-detail"
                rows={3}
                value={detail}
                onChange={(event) => setDetail(event.target.value)}
              />
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setAdding(null)}>
              {t`Cancel`}
            </Button>
            <Button
              disabled={!title.trim() || busy}
              onClick={() => adding && void add(adding)}
            >
              {busy && <Loader2 className="animate-spin" />}
              {t`Add`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
