/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plus, Trash2, Workflow } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Flow, FlowStep, Run, Taken } from "@api"
import { Badge } from "@/components/ui/badge"
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

export const Route = createFileRoute("/dashboard/flows")({
  component: FlowsRoute,
})

/** What can start a flow. The API refuses anything else, and says so. */
const TRIGGERS = [
  "form.submitted",
  "post.published",
  "post.unpublished",
  "order.paid",
  "order.fulfilled",
  "refund.made",
  "stock.low",
] as const

/** What a step can be. */
const KINDS = ["send_mail", "call_webhook", "wait", "add_to_list"] as const

type Draft = {
  kind: (typeof KINDS)[number]
  config: string
}

/**
 * What happens by itself when something happens.
 *
 * A flow is one trigger and a list of steps in order — no branches, no
 * conditions: two triggers is two flows, and a flow that reads like a program
 * is a program somebody has to debug through a web page.
 */
function FlowsRoute() {
  const { t } = useLingui()

  const [flows, setFlows] = React.useState<Flow[] | null>(null)
  const [open, setOpen] = React.useState<string | null>(null)
  const [steps, setSteps] = React.useState<Record<string, FlowStep[]>>({})
  const [runs, setRuns] = React.useState<Record<string, Run[]>>({})
  const [looking, setLooking] = React.useState<
    { run: Run; steps: Taken[] } | null
  >(null)

  const [making, setMaking] = React.useState(false)
  const [name, setName] = React.useState("")
  const [trigger, setTrigger] = React.useState<string>(TRIGGERS[0])
  const [drafts, setDrafts] = React.useState<Draft[]>([])
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    every("GET /api/flows")
      .then(setFlows)
      .catch((why: unknown) => {
        toast.error(said(why))
        setFlows([])
      })
  }, [])

  React.useEffect(load, [load])

  const look = async (flow: Flow) => {
    setOpen(open === flow.id ? null : flow.id)

    if (steps[flow.id]) return

    try {
      const whole = await api("GET /api/flows/{id}", { path: { id: flow.id } })
      const its = await every("GET /api/flows/{id}/runs", {
        path: { id: flow.id },
      })

      setSteps((held) => ({ ...held, [flow.id]: whole.steps }))
      setRuns((held) => ({ ...held, [flow.id]: its }))
    } catch (why) {
      toast.error(said(why))
    }
  }

  const switchIt = async (flow: Flow, active: boolean) => {
    try {
      await api("PATCH /api/flows/{id}", {
        path: { id: flow.id },
        body: { active },
      })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const remove = async (flow: Flow) => {
    try {
      await api("DELETE /api/flows/{id}", { path: { id: flow.id } })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const make = async () => {
    setBusy(true)

    try {
      await api("POST /api/flows", {
        body: {
          name: name.trim(),
          trigger,
          steps: drafts.map((step) => ({
            kind: step.kind,
            config: step.config.trim() ? JSON.parse(step.config) : {},
          })),
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
          : said(why),
      )
    } finally {
      setBusy(false)
    }
  }

  return (
    <>
      <div className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold">{t`Flows`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`What happens by itself when something happens: a form comes in, an order is paid. One trigger, then steps in order.`}
          </p>
        </div>
        <Button onClick={() => setMaking(true)}>
          <Plus /> {t`New flow`}
        </Button>
      </div>

      {flows === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : flows.length === 0 ? (
        <div className="rounded-xl border border-dashed border-border py-16 text-center">
          <Workflow className="mx-auto mb-3 size-8 text-muted-foreground" />
          <p className="text-sm text-muted-foreground">{t`No flows yet.`}</p>
        </div>
      ) : (
        <div className="flex max-w-3xl flex-col divide-y divide-border rounded-xl border border-border">
          {flows.map((flow) => (
            <div key={flow.id} className="px-4 py-3">
              <div className="flex flex-wrap items-center gap-3">
                <button
                  type="button"
                  className="min-w-0 flex-1 text-left"
                  onClick={() => void look(flow)}
                >
                  <p className="truncate text-sm font-medium">{flow.name}</p>
                  <p className="truncate font-mono text-xs text-muted-foreground">
                    {flow.trigger}
                  </p>
                </button>

                <Switch
                  checked={flow.active}
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
                <div className="mt-3 flex flex-col gap-3">
                  <div className="flex flex-col gap-1">
                    {(steps[flow.id] ?? []).map((step, index) => (
                      <p key={step.id} className="font-mono text-xs">
                        {index + 1}. {step.kind}{" "}
                        <span className="text-muted-foreground">
                          {JSON.stringify(step.config)}
                        </span>
                      </p>
                    ))}
                  </div>

                  <div className="flex flex-col gap-1">
                    <p className="text-xs font-medium">{t`Lately`}</p>
                    {(runs[flow.id] ?? []).length === 0 ? (
                      <p className="text-xs text-muted-foreground">
                        {t`It has not run yet.`}
                      </p>
                    ) : (
                      (runs[flow.id] ?? []).slice(0, 5).map((run) => (
                        <button
                          key={run.id}
                          type="button"
                          className="text-left text-xs hover:underline"
                          onClick={async () => {
                            try {
                              setLooking(
                                await api("GET /api/flows/runs/{id}", {
                                  path: { id: run.id },
                                }),
                              )
                            } catch (why) {
                              toast.error(said(why))
                            }
                          }}
                        >
                          <Badge
                            variant={
                              run.state === "failed" ? "secondary" : "default"
                            }
                          >
                            {run.state}
                          </Badge>{" "}
                          <span className="text-muted-foreground">
                            {new Date(run.started_at).toLocaleString()}
                            {run.failure ? ` · ${run.failure}` : ""}
                          </span>
                        </button>
                      ))
                    )}
                  </div>
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      <Dialog
        open={looking !== null}
        onOpenChange={(shown) => !shown && setLooking(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t`What this run did`}</DialogTitle>
            <DialogDescription>
              {t`Every step it took, in order, and what each one said.`}
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-2">
            {(looking?.steps ?? []).length === 0 ? (
              <p className="text-sm text-muted-foreground">
                {t`It stopped before any step ran.`}
              </p>
            ) : (
              looking?.steps.map((step) => (
                <div key={step.position} className="text-xs">
                  <Badge
                    variant={
                      step.outcome === "failed" ? "secondary" : "default"
                    }
                  >
                    {step.outcome}
                  </Badge>{" "}
                  <span className="font-mono">
                    {step.position + 1}. {step.kind ?? t`a step since removed`}
                  </span>
                  {typeof step.detail === "object" &&
                  step.detail !== null &&
                  "why" in step.detail ? (
                    <p className="text-muted-foreground">
                      {String((step.detail as { why: unknown }).why)}
                    </p>
                  ) : null}
                </div>
              ))
            )}
          </div>
        </DialogContent>
      </Dialog>

      <Credentials />

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
                onValueChange={(value) => setTrigger(value ?? TRIGGERS[0])}
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
                    setDrafts([...drafts, { kind: "send_mail", config: "{}" }])
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
                                  kind:
                                    (value as Draft["kind"]) ?? "send_mail",
                                }
                              : one,
                          ),
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
                    value={step.config}
                    onChange={(event) =>
                      setDrafts(
                        drafts.map((one, which) =>
                          which === index
                            ? { ...one, config: event.target.value }
                            : one,
                        ),
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
    </>
  )
}

/**
 * The secrets a flow uses, by name.
 *
 * A step says "use `stripe`" and this is where `stripe` is. Nothing here
 * answers with a secret — the name is the whole of what can be read back,
 * which is why there is a name at all.
 */
function Credentials() {
  const { t } = useLingui()

  const [held, setHeld] = React.useState<
    { name: string; updated_at: string }[] | null
  >(null)
  const [name, setName] = React.useState("")
  const [secret, setSecret] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    api("GET /api/flows/credentials")
      .then(setHeld)
      .catch((why: unknown) => {
        toast.error(said(why))
        setHeld([])
      })
  }, [])

  React.useEffect(load, [load])

  const keep = async () => {
    setBusy(true)

    try {
      await api("POST /api/flows/credentials", {
        body: { name: name.trim(), secret },
      })
      setName("")
      setSecret("")
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  const forget = async (one: string) => {
    try {
      await api("DELETE /api/flows/credentials/{name}", { path: { name: one } })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  return (
    <section className="mt-8 flex max-w-3xl flex-col gap-3 rounded-xl border border-border p-4">
      <h2 className="text-sm font-medium">{t`What a flow signs in with`}</h2>
      <p className="text-sm text-muted-foreground">
        {t`Kept sealed and never read back. A step names one; this is where the name points.`}
      </p>

      {(held ?? []).length > 0 && (
        <div className="flex flex-col divide-y divide-border rounded-lg border border-border">
          {(held ?? []).map((one) => (
            <div key={one.name} className="flex items-center gap-3 px-3 py-2">
              <span className="min-w-0 flex-1 truncate font-mono text-xs">
                {one.name}
              </span>
              <span className="text-xs text-muted-foreground">
                {new Date(one.updated_at).toLocaleDateString()}
              </span>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Remove`}
                onClick={() => void forget(one.name)}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      <form
        className="flex flex-wrap gap-2"
        onSubmit={(event) => {
          event.preventDefault()
          void keep()
        }}
      >
        <Input
          value={name}
          placeholder={t`What it is called`}
          className="w-48 font-mono"
          onChange={(event) => setName(event.target.value)}
        />
        <Input
          type="password"
          value={secret}
          placeholder={t`The secret`}
          className="min-w-48 flex-1"
          onChange={(event) => setSecret(event.target.value)}
        />
        <Button type="submit" disabled={!name.trim() || !secret || busy}>
          {busy && <Loader2 className="animate-spin" />}
          {t`Keep it`}
        </Button>
      </form>
    </section>
  )
}
