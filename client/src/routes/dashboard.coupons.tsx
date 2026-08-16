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
      toast.success(t`Removed`)
    } catch (why) {
      toast.error(said(why))
    }
  }

  const worth = (row: Coupon) =>
    row.percent !== null && row.percent !== undefined
      ? `%${row.percent}`
      : row.amount
        ? money(row.amount.minor, row.amount.currency)
        : ""

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
            {t`Checked twice: once when somebody types it into the basket, and again when the order is made.`}
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
              key={row.code}
              className="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-3"
            >
              <div className="min-w-0 flex-1 basis-40">
                <p className="font-mono text-sm font-medium">{row.code}</p>
                <p className="truncate text-xs text-muted-foreground">
                  {worth(row)}
                  {row.at_most_uses !== null &&
                    row.at_most_uses !== undefined && (
                      <> · {t`max ${row.at_most_uses} uses`}</>
                    )}
                  {row.expires_at && (
                    <>
                      {" · "}
                      {t`until ${new Date(row.expires_at).toLocaleDateString()}`}
                    </>
                  )}
                </p>
              </div>

              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Remove`}
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
  const [usesAllowed, setUsesAllowed] = React.useState("")
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
          code: code.trim().toUpperCase(),
          percent: kind === "percent" ? Number(amount) : null,
          amount_minor: kind === "amount" ? minor(amount) : null,
          currency: kind === "amount" ? "TRY" : null,
          at_most_uses: usesAllowed.trim() ? Number(usesAllowed) : null,
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
