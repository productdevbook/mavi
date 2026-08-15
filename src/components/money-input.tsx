import * as React from "react"

import { money } from "@/lib/money"
import { Input } from "@/components/ui/input"

/**
 * An amount typed the way people write money.
 *
 * "200" means two hundred, "199,90" and "199.90" both mean a shade under —
 * and what leaves this component is minor units, which is the only form an
 * amount ever takes on the wire. This exists because a field that wanted
 * minor units read "200" as two units and somebody typed 200000 to mean two
 * hundred.
 */
export function MoneyInput({
  id,
  currency,
  valueMinor,
  onChangeMinor,
  className,
  placeholder,
}: {
  id?: string
  currency: string
  valueMinor: number
  onChangeMinor: (minor: number) => void
  className?: string
  placeholder?: string
}) {
  const [text, setText] = React.useState(valueMinor ? major(valueMinor) : "")
  const [seen, setSeen] = React.useState(valueMinor)

  // An outside change (loading saved settings, a reset) rewrites the field;
  // retyping "1" while holding 100 does not. During render, the way React
  // documents for state derived from a prop.
  if (valueMinor !== seen) {
    setSeen(valueMinor)
    if (parseMajor(text) !== valueMinor) {
      setText(valueMinor ? major(valueMinor) : "")
    }
  }

  const shown = parseMajor(text)

  return (
    <div className={className}>
      <div className="relative">
        <Input
          id={id}
          inputMode="decimal"
          placeholder={placeholder ?? "0"}
          value={text}
          onChange={(event) => {
            const typed = event.target.value
            setText(typed)
            onChangeMinor(parseMajor(typed))
          }}
          className="pr-14"
        />
        <span className="pointer-events-none absolute inset-y-0 right-3 flex items-center text-sm text-muted-foreground">
          {currency}
        </span>
      </div>
      {shown > 0 && (
        <p className="mt-1 text-xs text-muted-foreground">
          = {money(shown, currency)}
        </p>
      )}
    </div>
  )
}

function major(minor: number): string {
  const whole = Math.trunc(minor / 100)
  const part = Math.abs(minor % 100)
  return part === 0 ? String(whole) : `${whole}.${String(part).padStart(2, "0")}`
}

/** "1.234,56", "1234.56", "1234" — all read as what a person meant. */
function parseMajor(text: string): number {
  const trimmed = text.trim().replace(/\s/g, "")
  if (!trimmed) return 0
  // The last separator is the decimal one; anything before it groups digits.
  const lastComma = trimmed.lastIndexOf(",")
  const lastDot = trimmed.lastIndexOf(".")
  const decimalAt = Math.max(lastComma, lastDot)
  let whole = trimmed
  let fraction = ""
  if (decimalAt >= 0) {
    const tail = trimmed.slice(decimalAt + 1)
    // Three or more digits after the only separator is grouping: "1.500".
    if (tail.length <= 2 || (lastComma >= 0 && lastDot >= 0)) {
      whole = trimmed.slice(0, decimalAt)
      fraction = tail
    }
  }
  whole = whole.replace(/[.,]/g, "")
  const negative = whole.startsWith("-")
  const digits = whole.replace(/[^0-9]/g, "")
  const cents = `${fraction.replace(/[^0-9]/g, "")}00`.slice(0, 2)
  if (!digits && !cents.trim()) return 0
  const minor = Number(digits || "0") * 100 + Number(cents)
  return negative ? -minor : minor
}
