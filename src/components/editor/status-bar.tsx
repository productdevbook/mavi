import type { Editor } from "@tiptap/core"
import { useEditorState } from "@tiptap/react"
import { Plural, Trans, useLingui } from "@lingui/react/macro"
import { Check, CloudUpload, Loader2 } from "lucide-react"

import { cn } from "@/lib/utils"
import { readingTimeMinutes } from "@/lib/editor-utils"

export type SaveState = "idle" | "saving" | "saved"

export function StatusBar({
  editor,
  saveState,
  savedAt,
  characterLimit,
}: {
  editor: Editor
  saveState: SaveState
  savedAt: Date | null
  characterLimit: number | null
}) {
  const { i18n } = useLingui()

  const stats = useEditorState({
    editor,
    selector: ({ editor: instance }) => {
      const { from, to } = instance.state.selection
      return {
        words: instance.storage.characterCount.words(),
        characters: instance.storage.characterCount.characters(),
        selected: from === to ? 0 : to - from,
        headings: instance.storage.tableOfContents.content.length,
      }
    },
  })

  const ratio = characterLimit ? stats.characters / characterLimit : 0
  const minutes = readingTimeMinutes(stats.words)

  return (
    // A phone shows the word count and whether the work is safe. The rest is
    // desk reading, and wrapping it onto three rows costs more screen than it
    // is worth to somebody typing with a keyboard over half the display.
    <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-border px-4 py-1.5 text-xs text-muted-foreground">
      <span className="tabular-nums">
        <strong className="font-medium text-foreground">{stats.words}</strong>{" "}
        <Plural value={stats.words} one="word" other="words" />
      </span>
      <span className="hidden tabular-nums sm:inline">
        <Plural
          value={stats.characters}
          one="# character"
          other="# characters"
        />
      </span>
      {stats.selected > 0 && (
        <span className="text-primary tabular-nums">
          <Plural
            value={stats.selected}
            one="# character selected"
            other="# characters selected"
          />
        </span>
      )}
      <span className="hidden sm:inline">
        <Plural value={minutes} one="# min read" other="# min read" />
      </span>
      <span className="hidden tabular-nums sm:inline">
        <Plural value={stats.headings} one="# heading" other="# headings" />
      </span>

      {characterLimit && (
        <span className="hidden items-center gap-1.5 sm:flex">
          <span
            className={cn(
              "h-1.5 w-24 overflow-hidden rounded-full bg-muted",
              ratio >= 1 && "bg-destructive/20"
            )}
          >
            <span
              className={cn(
                "block h-full rounded-full transition-all",
                ratio >= 1
                  ? "bg-destructive"
                  : ratio > 0.9
                    ? "bg-amber-500"
                    : "bg-primary"
              )}
              style={{ width: `${Math.min(100, ratio * 100)}%` }}
            />
          </span>
          <span className="tabular-nums">
            {stats.characters}/{characterLimit}
          </span>
        </span>
      )}

      <span className="ml-auto flex items-center gap-1.5">
        {saveState === "saving" && (
          <>
            <Loader2 className="size-3 animate-spin" /> <Trans>Saving…</Trans>
          </>
        )}
        {saveState === "saved" && (
          <>
            <Check className="size-3 text-emerald-500" />
            {savedAt ? (
              <Trans>
                Saved at{" "}
                {savedAt.toLocaleTimeString(i18n.locale, {
                  hour: "2-digit",
                  minute: "2-digit",
                })}
              </Trans>
            ) : (
              <Trans>Saved</Trans>
            )}
          </>
        )}
        {saveState === "idle" && (
          <>
            <CloudUpload className="size-3" /> <Trans>Unsaved changes</Trans>
          </>
        )}
      </span>
    </div>
  )
}
