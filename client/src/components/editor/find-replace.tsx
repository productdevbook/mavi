import * as React from "react"
import type { Editor } from "@tiptap/core"
import { useEditorState } from "@tiptap/react"
import { useLingui } from "@lingui/react/macro"
import {
  CaseSensitive,
  ChevronDown,
  ChevronUp,
  Regex,
  Replace,
  ReplaceAll,
  WholeWord,
  X,
} from "lucide-react"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ToolbarButton } from "@/components/editor/toolbar-button"

export function FindReplacePanel({
  editor,
  onClose,
}: {
  editor: Editor
  onClose: () => void
}) {
  const { t } = useLingui()
  const [search, setSearch] = React.useState(() => {
    const { from, to } = editor.state.selection
    const selected = editor.state.doc.textBetween(from, to, " ").trim()
    return selected && selected.length < 80 ? selected : ""
  })
  const [replace, setReplace] = React.useState("")
  const inputRef = React.useRef<HTMLInputElement>(null)

  const state = useEditorState({
    editor,
    selector: ({ editor: instance }) => ({
      total: instance.storage.searchReplace.results.length,
      index: instance.storage.searchReplace.resultIndex,
      caseSensitive: instance.storage.searchReplace.caseSensitive,
      wholeWord: instance.storage.searchReplace.wholeWord,
      useRegex: instance.storage.searchReplace.useRegex,
    }),
  })

  React.useEffect(() => {
    editor.commands.setSearchTerm(search)
    inputRef.current?.select()
    return () => {
      editor.commands.clearSearch()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor])

  React.useEffect(() => {
    editor.commands.setReplaceTerm(replace)
  }, [replace, editor])

  const onSearchChange = (value: string) => {
    setSearch(value)
    editor.commands.setSearchTerm(value)
  }

  const toggle = (key: "caseSensitive" | "wholeWord" | "useRegex") =>
    editor.commands.setSearchOptions({ [key]: !state[key] })

  return (
    <div className="absolute top-3 right-3 z-30 w-[22rem] rounded-xl border border-border bg-popover p-2.5 shadow-xl">
      <div className="flex items-center justify-between pb-2">
        <p className="text-xs font-medium text-muted-foreground">
          {t`Find and replace`}
        </p>
        <Button variant="ghost" size="icon-xs" onClick={onClose} aria-label={t`Close`}>
          <X />
        </Button>
      </div>

      <div className="flex items-center gap-1.5">
        <Input
          ref={inputRef}
          value={search}
          placeholder={t`Find…`}
          className="h-8"
          onChange={(event) => onSearchChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault()
              editor.commands.goToSearchResult(event.shiftKey ? -1 : 1)
            }
            if (event.key === "Escape") onClose()
          }}
        />
        <span
          className={cn(
            "w-16 shrink-0 text-center text-xs tabular-nums",
            state.total ? "text-muted-foreground" : "text-destructive"
          )}
        >
          {state.total ? `${state.index + 1}/${state.total}` : t`0 results`}
        </span>
        <ToolbarButton
          label={t`Previous`}
          keys="Shift+Enter"
          disabled={!state.total}
          onClick={() => editor.commands.goToSearchResult(-1)}
        >
          <ChevronUp />
        </ToolbarButton>
        <ToolbarButton
          label={t`Next`}
          keys="Enter"
          disabled={!state.total}
          onClick={() => editor.commands.goToSearchResult(1)}
        >
          <ChevronDown />
        </ToolbarButton>
      </div>

      <div className="mt-1.5 flex items-center gap-1.5">
        <Input
          value={replace}
          placeholder={t`Replace…`}
          className="h-8"
          onChange={(event) => setReplace(event.target.value)}
        />
        <ToolbarButton
          label={t`Replace`}
          disabled={!state.total}
          onClick={() => {
            editor.commands.replaceCurrent()
            editor.commands.goToSearchResult(0)
          }}
        >
          <Replace />
        </ToolbarButton>
        <ToolbarButton
          label={t`Replace all`}
          disabled={!state.total}
          onClick={() => editor.commands.replaceAll()}
        >
          <ReplaceAll />
        </ToolbarButton>
      </div>

      <div className="mt-1.5 flex items-center gap-1">
        <ToolbarButton
          label={t`Case sensitive`}
          active={state.caseSensitive}
          onClick={() => toggle("caseSensitive")}
        >
          <CaseSensitive />
        </ToolbarButton>
        <ToolbarButton
          label={t`Whole word`}
          active={state.wholeWord}
          onClick={() => toggle("wholeWord")}
        >
          <WholeWord />
        </ToolbarButton>
        <ToolbarButton
          label={t`Regular expression`}
          active={state.useRegex}
          onClick={() => toggle("useRegex")}
        >
          <Regex />
        </ToolbarButton>
      </div>
    </div>
  )
}
