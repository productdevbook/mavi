import * as React from "react"
import { Check } from "lucide-react"

import { cn } from "@/lib/utils"

/**
 * One numbered step of setting up mail.
 *
 * The screen this replaces was a column of equal-looking sections — a switch,
 * some fields, a save button, a test box, then the senders at the bottom —
 * which said nothing about what to do first. Somebody arriving to make their
 * contact form work had to already know the order.
 *
 * So the order is the page. A step that is finished collapses to its heading
 * and a tick; the one to do now is open. Nothing is hidden — a finished step
 * still opens — but only one of them is asking for anything.
 */
export function Step({
  number,
  title,
  summary,
  done,
  current,
  children,
}: {
  number: number
  title: string
  /** What it ended up being. Shown instead of the body once it is done. */
  summary?: React.ReactNode
  done: boolean
  current: boolean
  children: React.ReactNode
}) {
  // Follows whichever step is current until somebody says otherwise, and
  // then keeps what they said. Derived rather than kept in an effect, so it
  // cannot lag a render behind the step it is describing.
  const [chosen, setChosen] = React.useState<boolean | null>(null)
  const shown = chosen ?? current

  return (
    <div
      className={cn(
        "rounded-xl border transition-colors",
        current ? "border-primary/40 bg-card" : "border-border bg-card/40"
      )}
    >
      <button
        type="button"
        className="flex w-full items-center gap-3 p-4 text-left"
        onClick={() => setChosen(!shown)}
      >
        <span
          className={cn(
            "flex size-6 shrink-0 items-center justify-center rounded-full text-xs font-semibold",
            done
              ? "bg-emerald-600 text-white"
              : current
                ? "bg-primary text-primary-foreground"
                : "bg-muted text-muted-foreground"
          )}
        >
          {done ? <Check className="size-3.5" /> : number}
        </span>

        <span className="min-w-0 flex-1">
          <span className="block text-sm font-medium">{title}</span>
          {!shown && summary ? (
            <span className="block truncate text-sm text-muted-foreground">
              {summary}
            </span>
          ) : null}
        </span>
      </button>

      {shown ? (
        <div className="flex flex-col gap-4 border-t border-border p-4">
          {children}
        </div>
      ) : null}
    </div>
  )
}
