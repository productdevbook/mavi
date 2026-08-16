import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

/**
 * The currencies this panel knows how to write. A list rather than a text
 * field: a mistyped currency would quietly break every payment made in it,
 * and there is nothing to gain from being able to type one.
 */
const CURRENCIES = [
  ["USD", "$ USD"],
  ["EUR", "€ EUR"],
  ["GBP", "£ GBP"],
  ["TRY", "₺ TRY"],
] as const

export function CurrencySelect({
  id,
  value,
  onChange,
}: {
  id?: string
  value: string
  onChange: (currency: string) => void
}) {
  const held = value.trim().toUpperCase()
  const known = CURRENCIES.some(([code]) => code === held)

  return (
    <Select value={known ? held : "USD"} onValueChange={(code) => onChange(code ?? "USD")}>
      <SelectTrigger id={id} className="w-32">
        <SelectValue>
          {(code: string | null) =>
            CURRENCIES.find(([one]) => one === code)?.[1] ?? code ?? ""
          }
        </SelectValue>
      </SelectTrigger>
      <SelectContent>
        {CURRENCIES.map(([code, label]) => (
          <SelectItem key={code} value={code}>
            {label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
