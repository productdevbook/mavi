import * as React from "react"

import { WideSurface } from "@/lib/wide-surface"

export function WideSurfaceProvider({ children }: { children: React.ReactNode }) {
  const [wide, setWide] = React.useState(false)
  const value = React.useMemo(() => ({ wide, setWide }), [wide])
  return <WideSurface.Provider value={value}>{children}</WideSurface.Provider>
}
