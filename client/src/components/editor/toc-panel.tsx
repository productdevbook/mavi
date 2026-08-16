import type { Editor } from "@tiptap/core"
import type { TableOfContentData } from "@tiptap/extension-table-of-contents"
import { Trans, useLingui } from "@lingui/react/macro"
import { ListTree } from "lucide-react"

import { cn } from "@/lib/utils"

export function TocPanel({
  editor,
  items,
  onNavigate,
}: {
  editor: Editor
  items: TableOfContentData
  /// Set where the list is a drawer over the writing, which would otherwise
  /// stay in front of the heading it just jumped to.
  onNavigate?: () => void
}) {
  const { t } = useLingui()

  const goTo = (id: string, pos: number) => {
    const element = editor.view.dom.querySelector(`[data-toc-id="${id}"]`)
    element?.scrollIntoView({ behavior: "smooth", block: "start" })
    editor
      .chain()
      .setTextSelection(pos + 1)
      .focus()
      .run()
    onNavigate?.()
  }

  if (!items.length) {
    return (
      <div className="flex flex-col items-center gap-2 rounded-xl border border-dashed border-border px-4 py-8 text-center">
        <ListTree className="size-5 text-muted-foreground" />
        <p className="text-xs text-muted-foreground">
          <Trans>Headings you add will show up here.</Trans>
        </p>
      </div>
    )
  }

  return (
    <nav className="flex flex-col gap-0.5">
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          onClick={() => goTo(item.id, item.pos)}
          style={{ paddingInlineStart: `${(item.level - 1) * 12 + 8}px` }}
          className={cn(
            "group flex items-start gap-2 rounded-md py-1 pr-2 text-left text-sm transition-colors hover:bg-muted",
            item.isActive
              ? "bg-primary/10 font-medium text-primary"
              : "text-muted-foreground"
          )}
        >
          <span className="mt-0.5 w-6 shrink-0 text-[0.65rem] tabular-nums opacity-60">
            {item.itemIndex}
          </span>
          <span className="line-clamp-2 flex-1">
            {item.textContent || t`Untitled`}
          </span>
        </button>
      ))}
    </nav>
  )
}
