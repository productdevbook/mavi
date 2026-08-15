import * as React from "react"

import { cn } from "@/lib/utils"

/**
 * The shapes this panel draws, by hand.
 *
 * A charting library is a hundred kilobytes and a React-version problem twice
 * a year, and what is wanted here is a curve and a row of bars. The arithmetic
 * below is the whole of what a library would have done for them, and it is
 * shorter than its configuration object would have been.
 *
 * No pie and no ring on purpose: what gets divided up here is a handful of
 * names of different lengths with one big share and a tail, which a circle
 * renders as an unreadable fringe. Bars are read left to right and their
 * labels fit.
 *
 * Everything here draws in `currentColor` and inherits the panel's own
 * palette, so light and dark are already right and a site's accent colour
 * carries into the charts without a theme file.
 */

/** A day and a number, as the API writes it. */
export type Point = { on_day: string; count: number }

/** How the numbers are read aloud, for the label a screen reader gets. */
function summarise(points: Point[]): string {
  const total = points.reduce((sum, one) => sum + one.count, 0)
  return `${total} over ${points.length} days`
}

/**
 * A curve over time.
 *
 * Filled underneath rather than a bare line: at these sizes a line through
 * mostly-zero days is a flat wire, and the fill is what makes a quiet week
 * visibly quiet.
 */
export function Curve({
  points,
  className,
  height = 64,
}: {
  points: Point[]
  className?: string
  height?: number
}) {
  const width = 300
  const most = Math.max(1, ...points.map((one) => one.count))
  const step = points.length > 1 ? width / (points.length - 1) : width

  const at = (one: Point, index: number) => {
    const x = index * step
    // A day with nothing gets the floor, not the axis: a curve that touches
    // the bottom edge looks like missing data rather than a quiet Tuesday.
    const y = height - 2 - (one.count / most) * (height - 6)
    return `${x.toFixed(1)},${y.toFixed(1)}`
  }

  const line = points.map(at).join(" L ")
  const under = `M 0,${height} L ${line} L ${width},${height} Z`

  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      className={cn("h-16 w-full text-primary", className)}
      role="img"
      aria-label={summarise(points)}
    >
      <path d={under} fill="currentColor" opacity={0.12} />
      <path
        d={`M ${line}`}
        fill="none"
        stroke="currentColor"
        strokeWidth={1.5}
        vectorEffect="non-scaling-stroke"
        strokeLinejoin="round"
      />
    </svg>
  )
}

/**
 * Named parts of a whole, as rows.
 *
 * Rows and not a pie: the names are words of different lengths and the counts
 * are usually one big one and a tail, which a pie renders as a circle with an
 * unreadable fringe.
 */
export function Bars({
  slices,
  empty,
}: {
  slices: { name: string; count: number }[]
  empty: string
}) {
  const most = Math.max(1, ...slices.map((one) => one.count))

  if (slices.length === 0) {
    return <p className="text-sm text-muted-foreground">{empty}</p>
  }

  return (
    <div className="flex flex-col gap-2">
      {slices.map((slice) => (
        <div key={slice.name} className="flex items-center gap-3 text-sm">
          <span className="w-28 shrink-0 truncate" title={slice.name}>
            {slice.name}
          </span>
          <span className="h-2 flex-1 overflow-hidden rounded-full bg-muted">
            <span
              className="block h-full rounded-full bg-primary"
              style={{ width: `${Math.max(2, (slice.count / most) * 100)}%` }}
            />
          </span>
          <span className="w-10 shrink-0 text-right tabular-nums text-muted-foreground">
            {slice.count}
          </span>
        </div>
      ))}
    </div>
  )
}

/**
 * One number, and what it is.
 *
 * `hint` is for the thing the number does not say on its own — that eleven of
 * the ninety are unread, that four of six flows are switched on.
 */
export function Figure({
  label,
  value,
  hint,
  icon: Icon,
  tone,
}: {
  label: string
  value: string | number
  hint?: string
  icon?: React.ComponentType<{ className?: string }>
  /** `warn` for a number somebody should look at rather than admire. */
  tone?: "warn"
}) {
  return (
    <div className="rounded-xl border border-border p-4">
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        {Icon ? <Icon className="size-4" /> : null}
        <span className="truncate">{label}</span>
      </div>
      <p
        className={cn(
          "mt-1 text-2xl font-semibold tabular-nums",
          tone === "warn" && "text-destructive"
        )}
      >
        {value}
      </p>
      {hint ? (
        <p className="mt-0.5 truncate text-xs text-muted-foreground">{hint}</p>
      ) : null}
    </div>
  )
}

/** A panel with a heading, so the charts do not float. */
export function Panel({
  title,
  aside,
  children,
}: {
  title: string
  aside?: React.ReactNode
  children: React.ReactNode
}) {
  return (
    <section className="rounded-xl border border-border p-4">
      <div className="mb-3 flex items-baseline justify-between gap-3">
        <h2 className="text-sm font-medium">{title}</h2>
        {aside ? (
          <span className="text-xs text-muted-foreground">{aside}</span>
        ) : null}
      </div>
      {children}
    </section>
  )
}
