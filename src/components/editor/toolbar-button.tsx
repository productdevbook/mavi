import * as React from "react"

import { cn } from "@/lib/utils"
import { shortcut } from "@/lib/editor-utils"
import { Button } from "@/components/ui/button"
import { Kbd } from "@/components/ui/kbd"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"

interface ToolbarButtonProps extends React.ComponentProps<typeof Button> {
  label: string
  keys?: string
  active?: boolean
}

export function ToolbarButton({
  label,
  keys,
  active,
  className,
  size = "icon-sm",
  variant = "ghost",
  ...props
}: ToolbarButtonProps) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant={variant}
            size={size}
            aria-label={label}
            aria-pressed={active}
            data-active={active || undefined}
            className={cn(
              "text-muted-foreground data-[active]:bg-primary/10 data-[active]:text-primary",
              className
            )}
            {...props}
          />
        }
      />
      <TooltipContent>
        {label}
        {keys ? <Kbd>{shortcut(keys)}</Kbd> : null}
      </TooltipContent>
    </Tooltip>
  )
}

export function ToolbarSeparator() {
  return <span aria-hidden className="mx-0.5 h-5 w-px shrink-0 bg-border" />
}

export function ToolbarGroup({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return (
    <div
      className={cn("flex shrink-0 items-center gap-0.5", className)}
      {...props}
    />
  )
}
