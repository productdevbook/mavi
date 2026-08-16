/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plus, Tag, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { money } from "@/lib/money"
import type { Coupon } from "@api"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

export const Route = createFileRoute("/dashboard/coupons")({
  component: CouponsRoute,
})

function CouponsRoute() {
  const { t } = useLingui()
  const [rows, setRows] = React.useState<Coupon[] | null>(null)
  const [making, setMaking] = React.useState(false)

  const load = React.useCallback(() => {
    every("GET /api/coupons")
      .then(setRows)
      .catch((why: unknown) => {
        toast.error(said(why))
        setRows((held) => held ?? [])
      })
  }, [])

  React.useEffect(load, [load])

  const stop = async (row: Coupon) => {
    try {
      await api("DELETE /api/coupons/{code}", { path: { code: row.code } })
      load()
      toast.success(row.used > 0 ? t`Stopped` : t`Removed`)
    } catch (why) {
      toast.error(said(why))
    }
  }

  const worth = (row: Coupon) =>
    row.kind === "percent" ? `%${row.value}` : money(row.value, row.currency)

  if (making) {
    return (
      <NewCoupon
        onDone={() => {
          setMaking(false)
          load()
        }}
      />
    )
  }

  return (
    <>
      <div className="mb-6 flex flex-col items-start gap-4 sm:flex-row sm:justify-between">
        <div>
          <h1 className="text-lg font-semibold">{t`Discount codes`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`Checked twice: once when somebody types it into the basket, and again when the order is made — because a code with one use left can be used by somebody else in the minute in between.`}
          </p>
        </div>
        <Button className="shrink-0" onClick={() => setMaking(true)}>
          <Plus /> {t`New code`}
        </Button>
      </div>

      {rows === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : rows.length === 0 ? (
        <div className="rounded-xl border border-dashed border-border py-16 text-center">
          <Tag className="mx-auto mb-3 size-8 text-muted-foreground" />
          <p className="text-sm text-muted-foreground">{t`No codes yet.`}</p>
        </div>
      ) : (
        <div className="flex max-w-3xl flex-col divide-y divide-border rounded-xl border border-border">
          {rows.map((row) => (
            <div
              key={row.id}
              className="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-3"
            >
              <div className="min-w-0 flex-1 basis-40">
                <p className="font-mono text-sm font-medium">{row.code}</p>
                <p className="truncate text-xs text-muted-foreground">
                  {worth(row)}
                  {row.minimum_minor > 0 && (
                    <> · {t`from ${money(row.minimum_minor, row.currency)}`}</>
                  )}
                  {row.uses_allowed !== null &&
                    row.uses_allowed !== undefined && (
                      <>
                        {" · "}
                        {t`${row.used} of ${row.uses_allowed} used`}
                      </>
                    )}
                  {row.per_shopper !== null && row.per_shopper !== undefined && (
                    <> · {t`${row.per_shopper} per person`}</>
                  )}
                  {row.expires_at && (
                    <>
                      {" · "}
                      {t`until ${new Date(row.expires_at).toLocaleDateString()}`}
                    </>
                  )}
                </p>
              </div>

              <Badge variant="secondary">{t`${row.used} used`}</Badge>

              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={row.used > 0 ? t`Stop it` : t`Remove`}
                onClick={() => void stop(row)}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}
    </>
  )
}

function NewCoupon({ onDone }: { onDone: () => void }) {
  const { t } = useLingui()
  const [code, setCode] = React.useState("")
  const [kind, setKind] = React.useState<"percent" | "amount">("percent")
  const [amount, setAmount] = React.useState("")
  const [minimum, setMinimum] = React.useState("")
  const [usesAllowed, setUsesAllowed] = React.useState("")
  const [perShopper, setPerShopper] = React.useState("")
  const [expires, setExpires] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  /** What somebody typed as money, in the smallest unit. */
  const minor = (typed: string) => {
    const cleaned = typed.replace(",", ".").trim()

    return cleaned ? Math.round(Number(cleaned) * 100) : 0
  }

  const save = async () => {
    setBusy(true)

    try {
      await api("POST /api/coupons", {
        body: {
          code,
          kind,
          // A percentage is a whole percent; an amount is in the smallest unit.
          value: kind === "percent" ? Number(amount) : minor(amount),
          minimum_minor: minor(minimum),
          uses_allowed: usesAllowed.trim() ? Number(usesAllowed) : null,
          per_shopper: perShopper.trim() ? Number(perShopper) : null,
          expires_at: expires.trim() ? `${expires}T23:59:59Z` : null,
        },
      })
      toast.success(t`Saved`)
      onDone()
    } catch (why) {
      toast.error(said(why))
      setBusy(false)
    }
  }

  return (
    <div className="flex max-w-2xl flex-col gap-5">
      <div>
        <h1 className="text-lg font-semibold">{t`A new code`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`Typed without case: launch2026 and LAUNCH2026 are one code.`}
        </p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="code">{t`The code`}</Label>
          <Input
            id="code"
            value={code}
            onChange={(event) => setCode(event.target.value.toUpperCase())}
            placeholder="ACILIS"
            className="font-mono"
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="kind">{t`What it takes off`}</Label>
          <Select
            value={kind}
            onValueChange={(value) =>
              setKind((value as typeof kind) ?? "percent")
            }
          >
            <SelectTrigger id="kind">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="percent">{t`A percentage`}</SelectItem>
              <SelectItem value="amount">{t`An amount`}</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="amount">
            {kind === "percent" ? t`How many percent` : t`How much`}
          </Label>
          <Input
            id="amount"
            inputMode="decimal"
            value={amount}
            onChange={(event) => setAmount(event.target.value)}
            placeholder={kind === "percent" ? "15" : "50,00"}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="minimum">{t`Basket has to reach`}</Label>
          <Input
            id="minimum"
            inputMode="decimal"
            value={minimum}
            onChange={(event) => setMinimum(event.target.value)}
            placeholder={t`nothing`}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="uses-allowed">{t`Times in total`}</Label>
          <Input
            id="uses-allowed"
            inputMode="numeric"
            value={usesAllowed}
            onChange={(event) => setUsesAllowed(event.target.value)}
            placeholder={t`as often as they like`}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="per-shopper">{t`Times per person`}</Label>
          <Input
            id="per-shopper"
            inputMode="numeric"
            value={perShopper}
            onChange={(event) => setPerShopper(event.target.value)}
            placeholder={t`as often as they like`}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="expires">{t`Works until`}</Label>
          <Input
            id="expires"
            type="date"
            value={expires}
            onChange={(event) => setExpires(event.target.value)}
          />
        </div>
      </div>

      <div className="flex gap-2">
        <Button
          disabled={!code.trim() || !amount.trim() || busy}
          onClick={() => void save()}
        >
          {busy && <Loader2 className="animate-spin" />}
          {t`Save`}
        </Button>
        <Button variant="outline" onClick={onDone}>
          {t`Cancel`}
        </Button>
      </div>
    </div>
  )
}
