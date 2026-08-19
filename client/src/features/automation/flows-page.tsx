import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plus, Trash2, Workflow } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Flow, NewStep, Step } from "@api"
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

const TRIGGERS: Flow["trigger"][] = [
  "something_was_published",
  "somebody_filled_in_a_form",
  "an_order_was_paid_for",
  "an_order_went_out",
  "somebody_was_put_on_a_course",
  "somebody_finished_a_course",
]

const KINDS: Step["does"][] = [
  "send_a_letter",
  "call_an_address",
  "wait",
  "put_on_a_list",
]

type Draft = {
  does: Step["does"]
  told: string
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

  const load = React.useCallback(() => {
    every("flows.list")
      .then(setFlows)
      .catch((why: unknown) => {
        toast.error(said(why))
        setFlows([])
      })
  }, [])

  React.useEffect(load, [load])

  const switchIt = async (flow: Flow, on: boolean) => {
    try {
      await api("flows.change", {
        path: { id: flow.id },
        body: { on },
      })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const remove = async (flow: Flow) => {
    try {
      await api("flows.remove", { path: { id: flow.id } })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const make = async () => {
    setBusy(true)

    try {
      const steps: NewStep[] = drafts.map((step) => ({
        does: step.does,
        told: step.told.trim() ? JSON.parse(step.told) : {},
      }))

      await api("flows.make", {
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
          : said(why)
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
                  checked={flow.on}
                  onCheckedChange={(value) => void switchIt(flow, value)}
                />

                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t`Remove`}
                  onClick={() => void remove(flow)}
                >
                  <Trash2 />
                </Button>
              </div>

              {open === flow.id && (
                <div className="mt-3 flex flex-col gap-1">
                  {(flow.steps ?? []).map((step, index) => (
                    <p key={index} className="font-mono text-xs">
                      {index + 1}. {step.does}{" "}
                      <span className="text-muted-foreground">
                        {JSON.stringify(step.told)}
                      </span>
                    </p>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

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
                      { does: "send_a_letter", told: "{}" },
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
                      value={step.does}
                      onValueChange={(value) =>
                        setDrafts(
                          drafts.map((one, which) =>
                            which === index
                              ? {
                                  ...one,
                                  does:
                                    (value as Step["does"]) ?? "send_a_letter",
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
                    value={step.told}
                    onChange={(event) =>
                      setDrafts(
                        drafts.map((one, which) =>
                          which === index
                            ? { ...one, told: event.target.value }
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
