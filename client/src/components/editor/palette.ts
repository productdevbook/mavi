import { useLingui } from "@lingui/react/macro"

export interface Swatch {
  /// Names are translated, so they are not what a list is keyed on.
  id: string
  name: string
  value: string
}

// The macro rewrites `t` where it is written, not where the function it was
// handed to runs — a plain helper taking `t` as an argument produced twenty-six
// empty colour names, and twelve React children keyed on the same "".
export function useTextColors(): Swatch[] {
  const { t } = useLingui()
  return [
    { id: "default", name: t`Default`, value: "" },
    { id: "gray", name: t`Gray`, value: "#6b7280" },
    { id: "brown", name: t`Brown`, value: "#92400e" },
    { id: "orange", name: t`Orange`, value: "#ea580c" },
    { id: "yellow", name: t`Yellow`, value: "#ca8a04" },
    { id: "green", name: t`Green`, value: "#16a34a" },
    { id: "teal", name: t`Teal`, value: "#0891b2" },
    { id: "blue", name: t`Blue`, value: "#2563eb" },
    { id: "navy", name: t`Navy`, value: "#1e3a8a" },
    { id: "purple", name: t`Purple`, value: "#7c3aed" },
    { id: "pink", name: t`Pink`, value: "#db2777" },
    { id: "red", name: t`Red`, value: "#dc2626" },
  ]
}

export function useHighlightColors(): Swatch[] {
  const { t } = useLingui()
  return [
    { id: "yellow", name: t`Yellow`, value: "#fef08a" },
    { id: "green", name: t`Green`, value: "#bbf7d0" },
    { id: "blue", name: t`Blue`, value: "#bfdbfe" },
    { id: "purple", name: t`Purple`, value: "#e9d5ff" },
    { id: "pink", name: t`Pink`, value: "#fbcfe8" },
    { id: "orange", name: t`Orange`, value: "#fed7aa" },
    { id: "gray", name: t`Gray`, value: "#e5e7eb" },
    { id: "red", name: t`Red`, value: "#fecaca" },
  ]
}

export function useFontFamilies(): Swatch[] {
  const { t } = useLingui()
  return [
    { id: "default", name: t`Default`, value: "" },
    { id: "inter", name: "Inter", value: "'Inter Variable', sans-serif" },
    { id: "serif", name: t`Serif`, value: "Georgia, 'Times New Roman', serif" },
    {
      id: "mono",
      name: t`Mono`,
      value: "ui-monospace, 'SF Mono', Menlo, monospace",
    },
    { id: "system", name: t`System`, value: "system-ui, sans-serif" },
    { id: "cursive", name: t`Cursive`, value: "'Brush Script MT', cursive" },
  ]
}

export const FONT_SIZES = [
  "12px",
  "14px",
  "16px",
  "18px",
  "20px",
  "24px",
  "30px",
  "36px",
  "48px",
]

export const LINE_HEIGHTS = ["1", "1.25", "1.5", "1.75", "2", "2.5"]
