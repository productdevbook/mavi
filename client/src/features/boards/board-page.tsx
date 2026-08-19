import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { ArrowLeft, Loader2, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Board, Card as OneCard } from "@api"
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
  const [cards, setCards] = React.useState<OneCard[]>([])
  const [adding, setAdding] = React.useState<string | null>(null)
  const [title, setTitle] = React.useState("")
  const [detail, setDetail] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    Promise.all([
      api("boards.read", { path: { id: boardId } }),
      every("cards.list", { path: { id: boardId } }),
    ])
      .then(([b, c]) => {
        setBoard(b)
        setCards(c)
      })
      .catch((why: unknown) => {
        toast.error(said(why))
        setBoard(null)
      })
  }, [boardId])

  React.useEffect(load, [load])

  const add = async (stageId: string) => {
    setBusy(true)

    try {
      await api("cards.make", {
        path: { id: boardId },
        body: {
          stage: stageId,
          title: title.trim(),
          detail: detail.trim() || null,
        },
      })
      setAdding(null)
      setTitle("")
      setDetail("")
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  const move = async (card: OneCard, stageId: string) => {
    try {
      await api("cards.move", {
        path: { id: card.id },
        body: { stage: stageId },
      })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const remove = async (card: OneCard) => {
    try {
      await api("cards.remove", { path: { id: card.id } })
      load()
    } catch (why) {
      toast.error(said(why))
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
        {board.stages.map((stage) => {
          const stageCards = cards.filter((c) => c.stage_id === stage.id)

          return (
            <section
              key={stage.id}
              className="flex flex-col gap-2 rounded-xl border border-border p-3"
            >
              <div className="flex items-center justify-between">
                <h2 className="text-sm font-medium">{stage.name}</h2>
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

                  {card.detail && (
                    <p className="text-xs text-muted-foreground">
                      {card.detail}
                    </p>
                  )}

                  <Select
                    value={card.stage_id}
                    onValueChange={(value) =>
                      void move(card, value ?? card.stage_id)
                    }
                  >
                    <SelectTrigger size="sm">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {board.stages.map((one) => (
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
                onClick={() => setAdding(stage.id)}
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
