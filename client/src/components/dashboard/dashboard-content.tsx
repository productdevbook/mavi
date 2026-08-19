import type * as React from "react"

import { useWideSurface } from "@/lib/wide-surface"

/** The one place that decides whether a page is a reading surface or a canvas. */
export function DashboardContent({ children }: { children: React.ReactNode }) {
  const wide = useWideSurface()

  return (
    <div
      className={
        wide
          ? "w-full flex-1 px-4 py-5 sm:px-6 sm:py-6"
          : "mx-auto w-full max-w-5xl flex-1 px-4 py-6 sm:px-6 sm:py-8"
      }
    >
      {children}
    </div>
  )
}
