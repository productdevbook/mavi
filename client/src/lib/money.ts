/**
 * An amount, as somebody reads it.
 *
 * Money is kept and sent in the smallest unit everywhere — there is no float
 * anywhere near it — so this is the one place it becomes a string with a
 * separator in it.
 */
export function money(minor: number, currency: string): string {
  const sign = minor < 0 ? "-" : ""
  const whole = Math.floor(Math.abs(minor) / 100)
  const part = String(Math.abs(minor) % 100).padStart(2, "0")
  switch (currency.toUpperCase()) {
    case "TRY":
      return `${sign}₺${whole},${part}`
    case "GBP":
      return `${sign}£${whole}.${part}`
    case "USD":
      return `${sign}$${whole}.${part}`
    case "EUR":
      return `${sign}€${whole}.${part}`
    default:
      return `${sign}${whole}.${part} ${currency.toUpperCase()}`
  }
}

