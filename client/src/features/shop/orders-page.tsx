import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Loader2, PackageCheck, Receipt } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { money } from "@/lib/money"
import type { Order } from "@api"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  DashboardEmpty,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

/**
 * What has been bought.
 *
 * A card payment marks itself paid: the provider says so, signed, and nothing
 * here has to be pressed. What is here is for the money that arrives another
 * way — a transfer, cash on the day — and it is written down as somebody
 * saying so, which is what the record is for.
 */
export function OrdersPage() {
  const { t, i18n } = useLingui()
  const [rows, setRows] = React.useState<Order[] | null>(null)
  const [busy, setBusy] = React.useState<string | null>(null)
  const [open, setOpen] = React.useState<string | null>(null)
  const [items, setItems] = React.useState<Record<string, string>>({})

  const load = React.useCallback(() => {
    every("GET /api/orders")
      .then(setRows)
      .catch((why: unknown) => {
        toast.error(said(why))
        setRows((held) => held ?? [])
      })
  }, [])

  React.useEffect(load, [load])

  const states: Record<string, string> = {
    waiting: t`Waiting for payment`,
    paid: t`Paid`,
    sent: t`Sent`,
    called_off: t`Cancelled`,
    given_back: t`Refunded`,
  }

  const paid = async (id: string) => {
    if (
      !window.confirm(
        t`Say the money for this order has arrived? The record will say you said so.`
      )
    ) {
      return
    }

    setBusy(id)

    try {
      await api("POST /api/orders/{id}/moves", {
        path: { id },
        body: { to: "paid" },
      })
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(null)
    }
  }

  const refund = async (id: string) => {
    setBusy(id)

    try {
      await api("POST /api/orders/{id}/moves", {
        path: { id },
        body: { to: "given_back" },
      })
      load()
      toast.success(t`Refunded. What the provider does with it is theirs.`)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(null)
    }
  }

  const send = async (id: string) => {
    setBusy(id)

    try {
      await api("POST /api/orders/{id}/moves", {
        path: { id },
        body: { to: "sent" },
      })
      load()
      toast.success(t`Marked as sent`)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(null)
    }
  }

  // What was in it, asked for only when somebody opens one: a line per order in
  // the list is a query per order.
  const look = async (id: string) => {
    setOpen(open === id ? null : id)

    if (items[id]) {
      return
    }

    try {
      const whole = await api("GET /api/orders/{id}", { path: { id } })

      setItems((held) => ({
        ...held,
        [id]: whole.lines
          .map((line) => `${line.how_many}× ${line.name}`)
          .join(", "),
      }))
    } catch (why) {
      toast.error(said(why))
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`Orders`}
        description={t`A card payment says so itself, signed by the provider. Say it here only for money that arrived another way — it is written down as you saying so.`}
      />

      {rows === null ? (
        <DashboardLoading />
      ) : rows.length === 0 ? (
        <DashboardEmpty
          icon={Receipt}
          title={t`Nothing sold yet.`}
          description={t`Orders will appear here after a customer completes checkout.`}
        />
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {rows.map((row) => (
            <div key={row.id} className="px-4 py-3">
              <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
                <button
                  type="button"
                  className="min-w-0 basis-full text-left sm:flex-1 sm:basis-0"
                  onClick={() => void look(row.id)}
                >
                  <p className="text-sm font-medium">
                    #{row.number} · {row.email}
                  </p>
                  <p className="truncate text-xs text-muted-foreground">
                    {new Date(row.created_at).toLocaleString(i18n.locale, {
                      dateStyle: "medium",
                      timeStyle: "short",
                    })}
                    {items[row.id] ? ` · ${items[row.id]}` : ""}
                  </p>
                </button>

                <Badge variant={row.state === "paid" ? "default" : "secondary"}>
                  {states[row.state] ?? row.state}
                </Badge>

                <span className="text-sm font-medium">
                  {money(row.total.minor, row.total.currency)}
                </span>

                {row.state === "waiting" && (
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={busy === row.id}
                    onClick={() => void paid(row.id)}
                  >
                    {t`The money arrived`}
                  </Button>
                )}

                {(row.state === "paid" || row.state === "sent") && (
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={busy === row.id}
                    onClick={() => void refund(row.id)}
                  >
                    {t`Refund it`}
                  </Button>
                )}

                {row.state === "paid" && (
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={busy === row.id}
                    onClick={() => void send(row.id)}
                  >
                    {busy === row.id ? (
                      <Loader2 className="animate-spin" />
                    ) : (
                      <PackageCheck />
                    )}
                    {t`It has gone`}
                  </Button>
                )}
              </div>

              {open === row.id && items[row.id] && (
                <p className="mt-2 text-xs text-muted-foreground">
                  {items[row.id]}
                </p>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
