/**
 * What a shopper's browser is allowed to ask for.
 *
 * Deliberately small, and deliberately not the panel's own client: what is
 * served to somebody buying a thing should not carry the shape of the whole
 * administrative API.
 *
 * There is no account here and no basket on the server. A basket is a list in
 * this browser until somebody buys it, which is the honest shape of the thing:
 * nothing is reserved by putting it in one, and the shop is not keeping a
 * record of what somebody nearly bought.
 */

export class ShopError extends Error {
  status: number

  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function ask<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api${path}`, {
    credentials: "same-origin",
    headers: init?.body ? { "content-type": "application/json" } : {},
    ...init,
  })

  if (!response.ok) {
    const body = await response.json().catch(() => null)

    throw new ShopError(
      response.status,
      String(body?.error?.message ?? response.statusText),
    )
  }

  if (response.status === 204) {
    return undefined as T
  }

  return response.json() as Promise<T>
}

export interface Money {
  minor: number
  currency: string
}

export interface Product {
  id: string
  slug: string
  name: string
  description: string | null
  price: Money
  stock: number
}

export interface Order {
  id: string
  number: number
  state: string
  email: string
  total: Money
  created_at: string
}

export interface Placed {
  order: Order
  /** Where to go and pay, when the site has somewhere. */
  pay_at: string | null
}

export const products = () =>
  ask<{ items: Product[]; next: string | null }>("/sites/products")

export const order = (id: string) => ask<Order>(`/sites/orders/${id}`)

export const buy = (
  email: string,
  items: { product_id: string; quantity: number }[],
  coupon: string | null,
  key: string,
) =>
  ask<Placed>("/sites/checkout", {
    method: "POST",
    body: JSON.stringify({ email, items, coupon, idempotency_key: key }),
  })

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
const BASKET = "mavicms.basket"

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
  window.dispatchEvent(new Event("mavicms.basket"))
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
  const held = sessionStorage.getItem("mavicms.attempt")

  if (held) {
    return held
  }

  const made = crypto.randomUUID()

  sessionStorage.setItem("mavicms.attempt", made)

  return made
}

export function forgetAttempt() {
  sessionStorage.removeItem("mavicms.attempt")
}
