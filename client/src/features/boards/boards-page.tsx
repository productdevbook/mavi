import * as React from "react"
import { Link } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { KanbanSquare, Loader2, Plus, Trash2, X } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Board } from "@api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
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
import {
  DashboardEmpty,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

/**
 * What this site works through, and in what order.
 *
 * The stages are the site's to name. This screen exists because the first
 * version of the idea had them written into the software — six of them, named
 * after one agency's sales process — and every site got that whether it took
 * enrolment requests or repair jobs.
 */
export function BoardsPage() {
  const { t } = useLingui()

  const [boards, setBoards] = React.useState<Board[] | null>(null)
  const [making, setMaking] = React.useState(false)
  const [name, setName] = React.useState("")
  const [stages, setStages] = React.useState<string[]>([
    t`Came in`,
    t`Working on it`,
    t`Done`,
  ])
  const [busy, setBusy] = React.useState(false)
  const [going, setGoing] = React.useState<Board | null>(null)

  const load = React.useCallback(() => {
    every("GET /api/boards")
      .then(setBoards)
      .catch((why: unknown) => {
        toast.error(said(why))
        setBoards((held) => held ?? [])
      })
  }, [])

  React.useEffect(load, [load])

  const make = async () => {
    setBusy(true)

    try {
      await api("POST /api/boards", {
        body: {
          name: name.trim(),
          stages: stages.map((stage) => stage.trim()).filter(Boolean),
        },
      })
      setMaking(false)
      setName("")
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  const remove = async () => {
    if (!going) return

    try {
      await api("DELETE /api/boards/{id}", { path: { id: going.id } })
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setGoing(null)
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`Boards`}
        description={t`Anything this site works through in order: enquiries, repairs, applications. You name the stages.`}
        actions={
          <Button onClick={() => setMaking(true)}>
            <Plus /> {t`New board`}
          </Button>
        }
      />

      {boards === null ? (
        <DashboardLoading />
      ) : boards.length === 0 ? (
        <DashboardEmpty
          icon={KanbanSquare}
          title={t`No boards yet.`}
          description={t`Create a board to move work through stages.`}
          action={
            <Button onClick={() => setMaking(true)}>
              <Plus /> {t`New board`}
            </Button>
          }
        />
      ) : (
        <div className="flex max-w-2xl flex-col divide-y divide-border rounded-xl border border-border">
          {boards.map((board) => (
            <div key={board.id} className="flex items-center gap-3 px-4 py-3">
              <KanbanSquare className="size-4 shrink-0 text-muted-foreground" />
              <Link
                to="/dashboard/boards/$boardId"
                params={{ boardId: board.id }}
                className="min-w-0 flex-1 truncate text-sm font-medium hover:underline"
              >
                {board.name}
              </Link>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Remove`}
                onClick={() => setGoing(board)}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      <Dialog open={making} onOpenChange={setMaking}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t`A new board`}</DialogTitle>
            <DialogDescription>
              {t`What it is called, and the stages a card moves through. Both can be changed afterwards.`}
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-2">
              <Label htmlFor="board-name">{t`What it is called`}</Label>
              <Input
                id="board-name"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder={t`Enquiries`}
              />
            </div>

            <div className="flex flex-col gap-2">
              <Label>{t`Stages`}</Label>
              {stages.map((stage, index) => (
                <div key={index} className="flex gap-2">
                  <Input
                    value={stage}
                    onChange={(event) =>
                      setStages(
                        stages.map((one, which) =>
                          which === index ? event.target.value : one
                        )
                      )
                    }
                  />
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={t`Remove`}
                    onClick={() =>
                      setStages(stages.filter((_, which) => which !== index))
                    }
                  >
                    <X />
                  </Button>
                </div>
              ))}
              <Button
                variant="outline"
                size="sm"
                className="self-start"
                onClick={() => setStages([...stages, ""])}
              >
                <Plus /> {t`Another stage`}
              </Button>
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setMaking(false)}>
              {t`Cancel`}
            </Button>
            <Button disabled={!name.trim() || busy} onClick={() => void make()}>
              {busy && <Loader2 className="animate-spin" />}
              {t`Make it`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={going !== null}
        onOpenChange={(open) => !open && setGoing(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Remove this board?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`Its cards go with it.`}
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
    </div>
  )
}
