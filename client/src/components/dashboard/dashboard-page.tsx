import type * as React from "react"
import type { LucideIcon } from "lucide-react"

import { cn } from "@/lib/utils"

/** Shared page grammar for authenticated site screens. */
export function DashboardPageHeader({
  title,
  description,
  actions,
  className,
}: {
  title: string
  description?: string
  actions?: React.ReactNode
  className?: string
}) {
  return (
    <div
      className={cn(
        "flex flex-wrap items-start justify-between gap-4",
        className,
      )}
    >
      <div className="min-w-0">
        <h1 className="text-xl font-semibold tracking-tight">{title}</h1>
        {description ? (
          <p className="mt-1 max-w-2xl text-sm text-muted-foreground">
            {description}
          </p>
        ) : null}
      </div>
      {actions ? <div className="shrink-0">{actions}</div> : null}
    </div>
  )
}

export function DashboardLoading() {
  return (
    <div className="flex min-h-48 items-center justify-center" aria-busy="true">
      <div className="size-5 animate-spin rounded-full border-2 border-muted border-t-primary" />
    </div>
  )
}

export function DashboardError({ message }: { message: string }) {
  return (
    <p className="rounded-xl border border-destructive/40 bg-destructive/5 px-4 py-3 text-sm text-destructive">
      {message}
    </p>
  )
}

export function DashboardEmpty({
  icon: Icon,
  title,
  description,
  action,
}: {
  icon?: LucideIcon
  title: string
  description?: string
  action?: React.ReactNode
}) {
  return (
    <div className="flex min-h-48 flex-col items-center justify-center rounded-xl border border-dashed border-border px-6 py-10 text-center">
      {Icon ? <Icon className="mb-3 size-8 text-muted-foreground" /> : null}
      <p className="text-sm font-medium">{title}</p>
      {description ? (
        <p className="mt-1 max-w-md text-sm text-muted-foreground">
          {description}
        </p>
      ) : null}
      {action ? <div className="mt-4">{action}</div> : null}
    </div>
  )
}
