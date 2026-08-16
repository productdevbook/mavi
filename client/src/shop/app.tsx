import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import {
  CheckCircle2,
  Loader2,
  Minus,
  Plus,
  ShoppingBag,
  Trash2,
} from "lucide-react"

import * as shop from "@/shop/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

/**
 * Three screens and a path switch.
 *
 * The product pages are the site's own — built, static, indexed. These are the
 * three nobody wants to design and every shop needs: what is in the basket,
 * paying for it, and what happened to an order.
 */
type Where = { at: "basket" } | { at: "order"; id: string }

const ROOT = "/shop"

function read(): Where {
  const path = window.location.pathname
    .replace(ROOT, "")
    .replace(/^\/|\/$/g, "")

  const parts = path.split("/").filter(Boolean)

  if (parts[0] === "orders" && parts[1]) return { at: "order", id: parts[1] }

  return { at: "basket" }
}

function go(to: string) {
  window.history.pushState({}, "", `${ROOT}${to}`)
  window.dispatchEvent(new PopStateEvent("popstate"))
}

export function App() {
  const { t } = useLingui()
  const [where, setWhere] = React.useState<Where>(read)

  React.useEffect(() => {
    const onMove = () => setWhere(read())

    window.addEventListener("popstate", onMove)

    return () => window.removeEventListener("popstate", onMove)
  }, [])

  return (
    <div className="min-h-svh bg-background">
      <header className="flex items-center gap-3 border-b border-border px-4 py-3">
        <button
          className="text-sm font-medium"
          onClick={() => go("/")}
        >{t`Basket`}</button>
      </header>

      <main className="mx-auto w-full max-w-2xl px-4 py-6 sm:px-6 sm:py-10">
        {where.at === "basket" ? <Basket /> : <Order id={where.id} />}
      </main>
    </div>
  )
}

function Basket() {
  const { t } = useLingui()

  const [held, setHeld] = React.useState(shop.basket)
  const [products, setProducts] = React.useState<shop.Product[] | null>(null)
  const [email, setEmail] = React.useState("")
  const [coupon, setCoupon] = React.useState("")
  const [busy, setBusy] = React.useState(false)
  const [refused, setRefused] = React.useState("")

  React.useEffect(() => {
    const onChange = () => setHeld(shop.basket())

    window.addEventListener("mavi.basket", onChange)

    return () => window.removeEventListener("mavi.basket", onChange)
  }, [])

  React.useEffect(() => {
    shop
      .products()
      .then((page) => setProducts(page.items))
      .catch(() => setProducts([]))
  }, [])

  if (products === null) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const lines = held
    .map((one) => ({
      ...one,
      product: products.find((product) => product.id === one.product_id),
    }))
    .filter((one) => one.product)

  const total = lines.reduce(
    (all, line) => all + (line.product?.price.minor ?? 0) * line.quantity,
    0,
  )

  const currency = lines[0]?.product?.price.currency ?? "TRY"

  const buy = async () => {
    setBusy(true)
    setRefused("")

    try {
      const placed = await shop.buy(
        email.trim(),
        held,
        coupon.trim() || null,
        shop.attempt(),
      )

      shop.empty()
      shop.forgetAttempt()

      // Somewhere to pay, where the site has somewhere; otherwise the order
      // itself, which says what is owed and how it stands.
      if (placed.pay_at) {
        window.location.href = placed.pay_at
      } else {
        go(`/orders/${placed.order.id}`)
      }
    } catch (why) {
      setRefused(
        why instanceof shop.ShopError ? why.message : t`Something failed`,
      )
      setBusy(false)
    }
  }

  if (lines.length === 0) {
    return (
      <div className="rounded-xl border border-dashed border-border py-16 text-center">
        <ShoppingBag className="mx-auto mb-3 size-8 text-muted-foreground" />
        <p className="text-sm text-muted-foreground">{t`Nothing in it yet.`}</p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-5">
      <h1 className="text-lg font-semibold">{t`Basket`}</h1>

      <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
        {lines.map((line) => (
          <div key={line.product_id} className="flex items-center gap-3 px-4 py-3">
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-medium">
                {line.product?.name}
              </p>
              <p className="text-xs text-muted-foreground">
                {line.product && shop.money(line.product.price)}
              </p>
            </div>

            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t`One fewer`}
              onClick={() => shop.add(line.product_id, -1)}
            >
              <Minus />
            </Button>

            <span className="w-6 text-center text-sm tabular-nums">
              {line.quantity}
            </span>

            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t`One more`}
              onClick={() => shop.add(line.product_id, 1)}
            >
              <Plus />
            </Button>

            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t`Take it out`}
              onClick={() => shop.add(line.product_id, -line.quantity)}
            >
              <Trash2 />
            </Button>
          </div>
        ))}
      </div>

      <div className="flex items-center justify-between">
        <span className="text-sm text-muted-foreground">{t`Altogether`}</span>
        <span className="text-sm font-medium">
          {shop.money({ minor: total, currency })}
        </span>
      </div>

      <div className="flex flex-col gap-2">
        <Label htmlFor="shop-email">{t`Where to send the receipt`}</Label>
        <Input
          id="shop-email"
          type="email"
          autoComplete="email"
          value={email}
          onChange={(event) => setEmail(event.target.value)}
        />
      </div>

      <div className="flex flex-col gap-2">
        <Label htmlFor="shop-coupon">{t`A discount code, if you have one`}</Label>
        <Input
          id="shop-coupon"
          className="font-mono"
          value={coupon}
          onChange={(event) => setCoupon(event.target.value.toUpperCase())}
        />
      </div>

      {refused && (
        <p className="rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {refused}
        </p>
      )}

      <Button disabled={busy || !email.trim()} onClick={() => void buy()}>
        {busy && <Loader2 className="animate-spin" />}
        {t`Buy`}
      </Button>
    </div>
  )
}

function Order({ id }: { id: string }) {
  const { t } = useLingui()
  const [order, setOrder] = React.useState<shop.Order | null>(null)

  React.useEffect(() => {
    shop
      .order(id)
      .then(setOrder)
      .catch(() => setOrder(null))
  }, [id])

  if (!order) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const states: Record<string, string> = {
    pending: t`Waiting for payment`,
    paid: t`Paid`,
    fulfilled: t`Sent`,
    cancelled: t`Cancelled`,
    refunded: t`Refunded`,
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-2">
        <CheckCircle2 className="size-5 text-emerald-600" />
        <h1 className="text-lg font-semibold">{t`Order #${order.number}`}</h1>
      </div>

      <p className="text-sm text-muted-foreground">
        {states[order.state] ?? order.state} · {shop.money(order.total)}
      </p>

      <p className="text-xs text-muted-foreground">
        {t`Keep this address: it is how you look this order up again.`}
      </p>
    </div>
  )
}
