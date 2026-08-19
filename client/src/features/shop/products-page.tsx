import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Boxes, Loader2, Plus } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import { money } from "@/lib/money"
import type { Product, UpdateProduct } from "@api"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
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

type Currency = "TRY" | "EUR" | "USD" | "GBP"
const CURRENCIES: Currency[] = ["TRY", "EUR", "USD", "GBP"]

/** An address out of a name: lower-case, dashes for gaps. */
function slugged(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
}

/**
 * What this site sells, and how many are left.
 *
 * Stock is held when somebody checks out and let go if nobody pays, so what is
 * shown here is what is actually available rather than what has been ordered
 * and not yet paid for.
 */
export function ProductsPage() {
  const { t } = useLingui()

  const [products, setProducts] = React.useState<Product[] | null>(null)
  const [making, setMaking] = React.useState(false)
  const [name, setName] = React.useState("")
  const [price, setPrice] = React.useState("")
  const [currency, setCurrency] = React.useState<Currency>("TRY")
  const [stock, setStock] = React.useState("0")
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    every("shop.products.list")
      .then(setProducts)
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setProducts((held) => held ?? [])
      })
  }, [])

  React.useEffect(load, [load])

  /** What somebody typed as money, in the smallest unit. */
  const minor = (typed: string) => {
    const cleaned = typed.replace(",", ".").trim()

    return cleaned ? Math.round(Number(cleaned) * 100) : 0
  }

  const make = async () => {
    setBusy(true)

    try {
      await api("shop.products.create", {
        body: {
          slug: slugged(name),
          name: name.trim(),
          price: { minor: minor(price), currency },
          stock: Number(stock) || 0,
        },
      })

      setMaking(false)
      setName("")
      setPrice("")
      setStock("0")
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    } finally {
      setBusy(false)
    }
  }

  const change = async (product: Product, changes: UpdateProduct) => {
    try {
      await api("shop.products.update", {
        path: { id: product.id },
        body: changes,
      })
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`Products`}
        description={t`What this site sells. Stock is held while somebody pays and let go if they do not, so two people cannot buy the last one.`}
        actions={
          <Button onClick={() => setMaking(true)}>
            <Plus /> {t`New product`}
          </Button>
        }
      />

      {products === null ? (
        <DashboardLoading />
      ) : products.length === 0 ? (
        <DashboardEmpty
          icon={Boxes}
          title={t`Nothing for sale yet.`}
          description={t`Create a product before you start taking orders.`}
          action={
            <Button onClick={() => setMaking(true)}>
              <Plus /> {t`New product`}
            </Button>
          }
        />
      ) : (
        <div className="flex max-w-3xl flex-col divide-y divide-border rounded-xl border border-border">
          {products.map((product) => (
            <div
              key={product.id}
              className="flex flex-wrap items-center gap-3 px-4 py-3"
            >
              <div className="min-w-0 flex-1 basis-40">
                <p className="truncate text-sm font-medium">{product.name}</p>
                <p className="truncate font-mono text-xs text-muted-foreground">
                  {product.slug}
                </p>
              </div>

              <span className="text-sm font-medium">
                {money(product.price.minor, product.price.currency)}
              </span>

              <div className="flex items-center gap-2">
                <Label
                  htmlFor={`stock-${product.id}`}
                  className="text-xs text-muted-foreground"
                >
                  {t`Left`}
                </Label>
                <Input
                  id={`stock-${product.id}`}
                  inputMode="numeric"
                  className="h-8 w-20"
                  defaultValue={String(product.stock)}
                  onBlur={(event) => {
                    const wanted = Number(event.target.value)

                    if (wanted !== product.stock) {
                      void change(product, { stock: wanted })
                    }
                  }}
                />
              </div>

              <Badge variant={product.on_sale ? "default" : "secondary"}>
                {product.on_sale ? t`For sale` : t`Not for sale`}
              </Badge>

              <Switch
                checked={product.on_sale}
                onCheckedChange={(value) =>
                  void change(product, { on_sale: value })
                }
              />
            </div>
          ))}
        </div>
      )}

      <Dialog open={making} onOpenChange={setMaking}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t`A new product`}</DialogTitle>
            <DialogDescription>
              {t`The address it answers on is made from the name and never changes, because a front end will be asking for it.`}
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-2">
              <Label htmlFor="product-name">{t`What it is called`}</Label>
              <Input
                id="product-name"
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
              <p className="font-mono text-xs text-muted-foreground">
                /{slugged(name)}
              </p>
            </div>

            <div className="flex gap-2">
              <div className="flex flex-1 flex-col gap-2">
                <Label htmlFor="product-price">{t`What it costs`}</Label>
                <Input
                  id="product-price"
                  inputMode="decimal"
                  value={price}
                  onChange={(event) => setPrice(event.target.value)}
                  placeholder="50,00"
                />
              </div>

              <div className="flex w-32 flex-col gap-2">
                <Label htmlFor="product-currency">{t`In`}</Label>
                <Select
                  value={currency}
                  onValueChange={(value) =>
                    setCurrency((value as Currency) ?? "TRY")
                  }
                >
                  <SelectTrigger id="product-currency">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {CURRENCIES.map((one) => (
                      <SelectItem key={one} value={one}>
                        {one}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="product-stock">{t`How many there are`}</Label>
              <Input
                id="product-stock"
                inputMode="numeric"
                value={stock}
                onChange={(event) => setStock(event.target.value)}
              />
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setMaking(false)}>
              {t`Cancel`}
            </Button>
            <Button
              disabled={!name.trim() || !price.trim() || busy}
              onClick={() => void make()}
            >
              {busy && <Loader2 className="animate-spin" />}
              {t`Make it`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
