import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plus, Trash2, Workflow } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { Flow, StepKind, Trigger } from "@api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
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
import {
  DashboardEmpty,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

const TRIGGERS: Trigger[] = [
  "content_published",
  "form_submitted",
  "order_paid",
  "order_sent",
  "course_enrollment_created",
  "course_lesson_completed",
]

const KINDS: StepKind[] = ["send_mail", "webhook", "wait", "add_to_mail_list"]

type Draft = {
  kind: StepKind
  configText: string
}

export function FlowsPage() {
  const { t } = useLingui()

  const [flows, setFlows] = React.useState<Flow[] | null>(null)
  const [open, setOpen] = React.useState<string | null>(null)

  const [making, setMaking] = React.useState(false)
  const [name, setName] = React.useState("")
  const [trigger, setTrigger] = React.useState<Flow["trigger"]>(TRIGGERS[0])
  const [drafts, setDrafts] = React.useState<Draft[]>([])
  const [busy, setBusy] = React.useState(false)
  const [going, setGoing] = React.useState<Flow | null>(null)

  const load = React.useCallback(() => {
    every("automation.flows.list", { query: {} })
      .then(setFlows)
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setFlows([])
      })
  }, [])

  React.useEffect(load, [load])

  const switchIt = async (flow: Flow, on: boolean) => {
    try {
      await api("automation.flows.update", {
        path: { id: flow.id },
        body: { enabled: on },
      })
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  const remove = async () => {
    if (!going) return

    try {
      await api("automation.flows.delete", { path: { id: going.id } })
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    } finally {
      setGoing(null)
    }
  }

  const make = async () => {
    setBusy(true)

    try {
      const steps = drafts.map((step) => ({
        kind: step.kind,
        config: step.configText.trim() ? JSON.parse(step.configText) : {},
      }))

      await api("automation.flows.create", {
        body: {
          name: name.trim(),
          trigger,
          steps,
        },
      })

      setMaking(false)
      setName("")
      setDrafts([])
      load()
    } catch (why) {
      toast.error(
        why instanceof SyntaxError
          ? t`One of the steps is not written as JSON.`
          : apiMessage(why)
      )
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`Flows`}
        description={t`What happens by itself when something happens: a form comes in, an order is paid. One trigger, then steps in order.`}
        actions={
          <Button onClick={() => setMaking(true)}>
            <Plus /> {t`New flow`}
          </Button>
        }
      />

      {flows === null ? (
        <DashboardLoading />
      ) : flows.length === 0 ? (
        <DashboardEmpty
          icon={Workflow}
          title={t`No flows yet.`}
          description={t`Create a trigger and steps to automate work on this site.`}
          action={
            <Button onClick={() => setMaking(true)}>
              <Plus /> {t`New flow`}
            </Button>
          }
        />
      ) : (
        <div className="flex max-w-3xl flex-col divide-y divide-border rounded-xl border border-border">
          {flows.map((flow) => (
            <div key={flow.id} className="px-4 py-3">
              <div className="flex flex-wrap items-center gap-3">
                <button
                  type="button"
                  className="min-w-0 flex-1 text-left"
                  onClick={() => setOpen(open === flow.id ? null : flow.id)}
                >
                  <p className="truncate text-sm font-medium">{flow.name}</p>
                  <p className="truncate font-mono text-xs text-muted-foreground">
                    {flow.trigger}
                  </p>
                </button>

                <Switch
                  checked={flow.enabled}
                  onCheckedChange={(value) => void switchIt(flow, value)}
                />

                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t`Remove`}
                  onClick={() => setGoing(flow)}
                >
                  <Trash2 />
                </Button>
              </div>

              {open === flow.id && (
                <div className="mt-3 flex flex-col gap-1">
                  {(flow.steps ?? []).map((step, index) => (
                    <p key={index} className="font-mono text-xs">
                      {index + 1}. {step.kind}{" "}
                      <span className="text-muted-foreground">
                        {JSON.stringify(step.config)}
                      </span>
                    </p>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      <AlertDialog
        open={going !== null}
        onOpenChange={(open) => !open && setGoing(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Move this flow to the bin?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`It stops running and can be restored from the bin with its previous enabled state.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void remove()}>
              {t`Move to bin`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <Dialog open={making} onOpenChange={setMaking}>
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t`A new flow`}</DialogTitle>
            <DialogDescription>
              {t`What starts it, and what it does. Each step's settings are written as JSON, which is what the API keeps.`}
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-2">
              <Label htmlFor="flow-name">{t`What it is called`}</Label>
              <Input
                id="flow-name"
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="flow-trigger">{t`What starts it`}</Label>
              <Select
                value={trigger}
                onValueChange={(value) =>
                  setTrigger((value as Flow["trigger"]) ?? TRIGGERS[0])
                }
              >
                <SelectTrigger id="flow-trigger">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {TRIGGERS.map((one) => (
                    <SelectItem key={one} value={one}>
                      {one}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="flex flex-col gap-2">
              <div className="flex items-center justify-between">
                <Label>{t`Steps`}</Label>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() =>
                    setDrafts([
                      ...drafts,
                      { kind: "send_mail", configText: "{}" },
                    ])
                  }
                >
                  <Plus /> {t`Another step`}
                </Button>
              </div>

              {drafts.map((step, index) => (
                <div
                  key={index}
                  className="flex flex-col gap-2 rounded-lg border border-border p-2"
                >
                  <div className="flex gap-2">
                    <Select
                      value={step.kind}
                      onValueChange={(value) =>
                        setDrafts(
                          drafts.map((one, which) =>
                            which === index
                              ? {
                                  ...one,
                                  kind: (value as StepKind) ?? "send_mail",
                                }
                              : one
                          )
                        )
                      }
                    >
                      <SelectTrigger className="w-44">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {KINDS.map((one) => (
                          <SelectItem key={one} value={one}>
                            {one}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>

                    <Button
                      variant="ghost"
                      size="icon"
                      aria-label={t`Remove`}
                      onClick={() =>
                        setDrafts(drafts.filter((_, which) => which !== index))
                      }
                    >
                      <Trash2 />
                    </Button>
                  </div>

                  <Textarea
                    rows={2}
                    className="font-mono text-xs"
                    value={step.configText}
                    onChange={(event) =>
                      setDrafts(
                        drafts.map((one, which) =>
                          which === index
                            ? { ...one, configText: event.target.value }
                            : one
                        )
                      )
                    }
                  />
                </div>
              ))}
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
    </div>
  )
}
