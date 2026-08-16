/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { ArrowLeft, Loader2, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { money } from "@/lib/money"
import type { Card as OneCard, Full, Note } from "@api"
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

export const Route = createFileRoute("/dashboard/boards_/$boardId")({
  component: BoardRoute,
})

/**
 * One board, stage by stage.
 *
 * A card moves by being told which stage it is in rather than by being
 * dragged: the position is a number the API keeps, and a screen that could
 * only move things by dragging is a screen that cannot be used on a phone.
 */
function BoardRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const { boardId } = Route.useParams()

  const [board, setBoard] = React.useState<Full | null>(null)
  const [adding, setAdding] = React.useState<string | null>(null)
  const [title, setTitle] = React.useState("")
  const [detail, setDetail] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    api("GET /api/boards/{id}", { path: { id: boardId } })
      .then(setBoard)
      .catch((why: unknown) => {
        toast.error(said(why))
        setBoard(null)
      })
  }, [boardId])

  React.useEffect(load, [load])

  const add = async (stageId: string) => {
    setBusy(true)

    try {
      await api("POST /api/boards/{id}/cards", {
        path: { id: boardId },
        body: {
          stage_id: stageId,
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
      await api("PATCH /api/cards/{id}", {
        path: { id: card.id },
        body: { stage_id: stageId },
      })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const [reading, setReading] = React.useState<string | null>(null)
  const [notes, setNotes] = React.useState<Record<string, Note[]>>({})
  const [note, setNote] = React.useState("")

  const look = async (card: OneCard) => {
    setReading(reading === card.id ? null : card.id)

    if (notes[card.id]) return

    try {
      const found = await every("GET /api/cards/{id}/notes", {
        path: { id: card.id },
      })

      setNotes((held) => ({ ...held, [card.id]: found }))
    } catch (why) {
      toast.error(said(why))
    }
  }

  const write = async (card: OneCard) => {
    if (!note.trim()) return

    try {
      await api("POST /api/cards/{id}/notes", {
        path: { id: card.id },
        body: { body: note.trim() },
      })

      setNote("")
      setNotes((held) => {
        const rest = { ...held }

        delete rest[card.id]

        return rest
      })
      void look(card)
    } catch (why) {
      toast.error(said(why))
    }
  }

  const remove = async (card: OneCard) => {
    try {
      await api("DELETE /api/cards/{id}", { path: { id: card.id } })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  if (!board) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <>
      <Button
        variant="ghost"
        size="sm"
        className="mb-4 -ml-2"
        onClick={() => void navigate({ to: "/dashboard/boards" })}
      >
        <ArrowLeft /> {t`Boards`}
      </Button>

      <input
        className="mb-6 w-full bg-transparent text-lg font-semibold outline-none"
        defaultValue={board.board.name}
        aria-label={t`What this board is called`}
        onBlur={(event) => {
          const wanted = event.target.value.trim()

          if (wanted && wanted !== board.board.name) {
            void api("PATCH /api/boards/{id}", {
              path: { id: boardId },
              body: { name: wanted },
            })
              .then(load)
              .catch((why: unknown) => toast.error(said(why)))
          }
        }}
      />

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {board.stages.map((stage) => (
          <section
            key={stage.id}
            className="flex flex-col gap-2 rounded-xl border border-border p-3"
          >
            <div className="flex items-center justify-between">
              <h2 className="text-sm font-medium">{stage.name}</h2>
              <span className="text-xs text-muted-foreground">
                {stage.cards.length}
              </span>
            </div>

            {stage.cards.map((card) => (
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
                  <p className="text-xs text-muted-foreground">{card.detail}</p>
                )}

                {card.value && (
                  <p className="text-xs font-medium">
                    {money(card.value.minor, card.value.currency)}
                  </p>
                )}

                <button
                  type="button"
                  className="self-start text-xs text-muted-foreground hover:underline"
                  onClick={() => void look(card)}
                >
                  {t`Notes`}
                </button>

                {reading === card.id && (
                  <div className="flex flex-col gap-1">
                    {(notes[card.id] ?? []).map((one) => (
                      <p key={one.id} className="text-xs text-muted-foreground">
                        {one.body}
                      </p>
                    ))}

                    <form
                      className="flex gap-1"
                      onSubmit={(event) => {
                        event.preventDefault()
                        void write(card)
                      }}
                    >
                      <Input
                        value={note}
                        placeholder={t`Something worth remembering`}
                        className="h-8"
                        onChange={(event) => setNote(event.target.value)}
                      />
                      <Button type="submit" size="sm" variant="outline">
                        {t`Add`}
                      </Button>
                    </form>
                  </div>
                )}

                <Select
                  value={card.stage_id}
                  onValueChange={(value) => void move(card, value ?? card.stage_id)}
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
        ))}
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
    </>
  )
}
