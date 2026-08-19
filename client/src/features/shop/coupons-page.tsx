import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plus, Tag, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
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
import {
  DashboardEmpty,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

export function CouponsPage() {
  const { t } = useLingui()
  const [rows, setRows] = React.useState<Coupon[] | null>(null)
  const [making, setMaking] = React.useState(false)

  const load = React.useCallback(() => {
    every("shop.coupons.list")
      .then(setRows)
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setRows((held) => held ?? [])
      })
  }, [])

  React.useEffect(load, [load])

  const stop = async (row: Coupon) => {
    try {
      await api("shop.coupons.delete", { path: { id: row.id } })
      load()
      toast.success(t`Removed`)
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  const worth = (row: Coupon) =>
    row.percent !== null && row.percent !== undefined
      ? `%${row.percent}`
      : moneyAmount(row.amount)

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
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`Discount codes`}
        description={t`Checked twice: once when somebody types it into the basket, and again when the order is made.`}
        actions={
          <Button className="shrink-0" onClick={() => setMaking(true)}>
            <Plus /> {t`New code`}
          </Button>
        }
      />

      {rows === null ? (
        <DashboardLoading />
      ) : rows.length === 0 ? (
        <DashboardEmpty
          icon={Tag}
          title={t`No codes yet.`}
          description={t`Create a code to offer a discount during checkout.`}
          action={
            <Button onClick={() => setMaking(true)}>
              <Plus /> {t`New code`}
            </Button>
          }
        />
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
                  {row.max_uses !== null && row.max_uses !== undefined && (
                    <> · {t`max ${row.max_uses} uses`}</>
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
    </div>
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
      await api("shop.coupons.create", {
        body: {
          code: code.trim().toUpperCase(),
          percent: kind === "percent" ? Number(amount) : null,
          amount_minor: kind === "amount" ? minor(amount) : null,
          currency: kind === "amount" ? "TRY" : null,
          max_uses: usesAllowed.trim() ? Number(usesAllowed) : null,
          expires_at: expires.trim() ? `${expires}T23:59:59Z` : null,
        },
      })
      toast.success(t`Saved`)
      onDone()
    } catch (why) {
      toast.error(apiMessage(why))
      setBusy(false)
    }
  }

  return (
    <div className="flex max-w-2xl flex-col gap-5">
      <DashboardPageHeader
        title={t`A new code`}
        description={t`Typed without case: launch2026 and LAUNCH2026 are one code.`}
      />

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

function moneyAmount(value: unknown): string {
  if (!value || typeof value !== "object") {
    return ""
  }

  const amount = value as { minor?: unknown; currency?: unknown }

  return typeof amount.minor === "number" && typeof amount.currency === "string"
    ? money(amount.minor, amount.currency)
    : ""
}
