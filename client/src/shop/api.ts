/** Public storefront access through the canonical generated Mavi contract. */

import {
  MaviApiError,
  MaviClient,
} from "@api"
import type {
  CheckoutReceipt,
  Money,
  OperationArguments,
  OperationName,
  OperationResponses,
  PublicProduct,
} from "@api"

type PublicOperation = Extract<
  OperationName,
  "shop.public.products.list" | "shop.public.orders.checkout"
>

export class ShopError extends Error {
  status: number

  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function ask<Name extends PublicOperation>(
  operation: Name,
  args: OperationArguments[Name],
): Promise<OperationResponses[Name]> {
  try {
    return await new MaviClient({ baseUrl: window.location.origin }).call(
      operation,
      args,
    )
  } catch (error) {
    if (error instanceof MaviApiError) {
      throw new ShopError(
        error.status,
        error.payload?.error.message ?? error.message,
      )
    }
    throw error
  }
}

export type { CheckoutReceipt, Money }
export type Product = PublicProduct

export const products = () =>
  ask("shop.public.products.list", { query: { limit: 100 } })

export const buy = (
  email: string,
  items: { product_id: string; quantity: number }[],
  coupon: string | null,
  key: string,
) =>
  ask("shop.public.orders.checkout", {
    body: {
      email,
      items,
      coupon_code: coupon,
      idempotency_key: key,
    },
  })

const RECEIPT = "mavi.shop.receipt"

export function saveReceipt(value: CheckoutReceipt): void {
  sessionStorage.setItem(RECEIPT, JSON.stringify(value))
}

export function receipt(id: string): CheckoutReceipt | null {
  try {
    const value = JSON.parse(sessionStorage.getItem(RECEIPT) ?? "null") as
      | CheckoutReceipt
      | null
    return value?.id === id ? value : null
  } catch {
    return null
  }
}

/** An amount, as somebody reads it. */
export function money(amount: Money): string {
  const whole = Math.floor(Math.abs(amount.minor) / 100)
  const part = String(Math.abs(amount.minor) % 100).padStart(2, "0")
  const sign = amount.minor < 0 ? "-" : ""

  switch (amount.currency.toUpperCase()) {
    case "TRY":
      return `${sign}₺${whole},${part}`
    case "GBP":
      return `${sign}£${whole}.${part}`
    case "USD":
      return `${sign}$${whole}.${part}`
    case "EUR":
      return `${sign}€${whole}.${part}`
    default:
      return `${sign}${whole}.${part} ${amount.currency.toUpperCase()}`
  }
}

/** What is in the basket, kept in this browser and nowhere else. */
const BASKET = "mavi.basket"

export interface Held {
  product_id: string
  quantity: number
}

export function basket(): Held[] {
  try {
    const held = JSON.parse(localStorage.getItem(BASKET) ?? "[]")

    return Array.isArray(held) ? (held as Held[]) : []
  } catch {
    return []
  }
}

export function keep(items: Held[]) {
  localStorage.setItem(BASKET, JSON.stringify(items))
  window.dispatchEvent(new Event("mavi.basket"))
}

export function add(productId: string, quantity = 1) {
  const held = basket()
  const already = held.find((one) => one.product_id === productId)

  if (already) {
    already.quantity += quantity
  } else {
    held.push({ product_id: productId, quantity })
  }

  keep(held.filter((one) => one.quantity > 0))
}

export function empty() {
  keep([])
}

/**
 * The same attempt twice is one order.
 *
 * Kept in the browser so that a refresh mid-payment is the same attempt rather
 * than a second one — the API decides, and this is what it decides on.
 */
export function attempt(): string {
  const held = sessionStorage.getItem("mavi.attempt")

  if (held) {
    return held
  }

  const made = crypto.randomUUID()

  sessionStorage.setItem("mavi.attempt", made)

  return made
}

export function forgetAttempt() {
  sessionStorage.removeItem("mavi.attempt")
}
