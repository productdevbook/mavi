import * as React from "react"
import type { Editor } from "@tiptap/core"
import type { EditorState } from "@tiptap/pm/state"
import { CellSelection } from "@tiptap/pm/tables"
import { BubbleMenu } from "@tiptap/react/menus"
import { useEditorState } from "@tiptap/react"
import { useLingui } from "@lingui/react/macro"
import {
  AlignCenter,
  AlignLeft,
  AlignRight,
  Bold,
  Braces,
  Check,
  Columns3,
  Copy,
  ExternalLink,
  Highlighter,
  Italic,
  Link2,
  Link2Off,
  Maximize2,
  Merge,
  Pencil,
  Rows3,
  Split,
  Strikethrough,
  Trash2,
  Underline,
} from "lucide-react"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  ToolbarButton,
  ToolbarSeparator,
} from "@/components/editor/toolbar-button"
import { useHighlightColors } from "@/components/editor/palette"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"

const panelClass =
  "flex items-center gap-0.5 rounded-xl border border-border bg-popover p-1 shadow-lg"

const TOP_OPTIONS = { placement: "top", offset: 8 } as const
const BOTTOM_OPTIONS = { placement: "bottom", offset: 8 } as const
const TABLE_OPTIONS = { placement: "top", offset: 12 } as const

function showOnTextSelection({
  editor,
  state,
  from,
  to,
}: {
  editor: Editor
  state: EditorState
  from: number
  to: number
}) {
  if (from === to) return false
  if (editor.isActive("image") || editor.isActive("codeBlock")) return false
  if (state.selection instanceof CellSelection) return false
  return state.doc.textBetween(from, to, " ").trim().length > 0
}

function showOnCollapsedLink({
  editor,
  state,
}: {
  editor: Editor
  state: EditorState
}) {
  return editor.isActive("link") && state.selection.empty
}

function showOnImage({ editor }: { editor: Editor }) {
  return editor.isActive("image")
}

function showOnTable({ editor }: { editor: Editor }) {
  return editor.isActive("table")
}

export function TextBubbleMenu({ editor }: { editor: Editor }) {
  const { t } = useLingui()
  const HIGHLIGHT_COLORS = useHighlightColors()

  const state = useEditorState({
    editor,
    selector: ({ editor: instance }) => ({
      bold: instance.isActive("bold"),
      italic: instance.isActive("italic"),
      underline: instance.isActive("underline"),
      strike: instance.isActive("strike"),
      code: instance.isActive("code"),
      link: instance.isActive("link"),
      highlight: instance.isActive("highlight"),
    }),
  })

  return (
    <BubbleMenu
      editor={editor}
      pluginKey="textBubbleMenu"
      updateDelay={100}
      options={TOP_OPTIONS}
      shouldShow={showOnTextSelection}
      className={panelClass}
    >
      <ToolbarButton
        label={t`Bold`}
        keys="Mod+B"
        active={state.bold}
        onClick={() => editor.chain().focus().toggleBold().run()}
      >
        <Bold />
      </ToolbarButton>
      <ToolbarButton
        label={t`Italic`}
        keys="Mod+I"
        active={state.italic}
        onClick={() => editor.chain().focus().toggleItalic().run()}
      >
        <Italic />
      </ToolbarButton>
      <ToolbarButton
        label={t`Underline`}
        keys="Mod+U"
        active={state.underline}
        onClick={() => editor.chain().focus().toggleUnderline().run()}
      >
        <Underline />
      </ToolbarButton>
      <ToolbarButton
        label={t`Strikethrough`}
        active={state.strike}
        onClick={() => editor.chain().focus().toggleStrike().run()}
      >
        <Strikethrough />
      </ToolbarButton>
      <ToolbarButton
        label={t`Code`}
        active={state.code}
        onClick={() => editor.chain().focus().toggleCode().run()}
      >
        <Braces />
      </ToolbarButton>

      <ToolbarSeparator />

      <Popover>
        <PopoverTrigger
          render={
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t`Highlight`}
              data-active={state.highlight || undefined}
              className="text-muted-foreground data-[active]:bg-primary/10 data-[active]:text-primary"
            />
          }
        >
          <Highlighter />
        </PopoverTrigger>
        <PopoverContent className="w-48" align="center">
          <div className="grid grid-cols-4 gap-1">
            {HIGHLIGHT_COLORS.map((swatch) => (
              <button
                key={swatch.id}
                type="button"
                title={swatch.name}
                aria-label={swatch.name}
                onClick={() =>
                  editor
                    .chain()
                    .focus()
                    .setHighlight({ color: swatch.value })
                    .run()
                }
                className="h-6 rounded-md border border-border"
                style={{ backgroundColor: swatch.value }}
              />
            ))}
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => editor.chain().focus().unsetHighlight().run()}
          >
            {t`Remove`}
          </Button>
        </PopoverContent>
      </Popover>

      <LinkEditor editor={editor} active={state.link} />
    </BubbleMenu>
  )
}

function LinkEditor({ editor, active }: { editor: Editor; active: boolean }) {
  const { t } = useLingui()
  const [open, setOpen] = React.useState(false)
  const [href, setHref] = React.useState("")
  const [wasOpen, setWasOpen] = React.useState(false)

  if (open !== wasOpen) {
    setWasOpen(open)
    if (open) setHref((editor.getAttributes("link").href as string) ?? "")
  }

  const apply = () => {
    const value = href.trim()
    if (!value) {
      editor.chain().focus().extendMarkRange("link").unsetLink().run()
    } else {
      editor
        .chain()
        .focus()
        .extendMarkRange("link")
        .setLink({ href: value })
        .run()
    }
    setOpen(false)
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t`Link`}
            data-active={active || undefined}
            className="text-muted-foreground data-[active]:bg-primary/10 data-[active]:text-primary"
          />
        }
      >
        <Link2 />
      </PopoverTrigger>
      <PopoverContent className="w-72" align="center">
        <div className="flex items-center gap-1.5">
          <Input
            autoFocus
            value={href}
            placeholder="https://example.com"
            onChange={(event) => setHref(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault()
                apply()
              }
            }}
            className="h-8"
          />
          <Button size="icon-sm" onClick={apply} aria-label={t`Apply`}>
            <Check />
          </Button>
        </div>
        {active && (
          <div className="flex items-center gap-1">
            <Button
              variant="ghost"
              size="sm"
              className="flex-1 justify-start text-muted-foreground"
              onClick={() => {
                editor.chain().focus().extendMarkRange("link").unsetLink().run()
                setOpen(false)
              }}
            >
              <Link2Off /> {t`Remove`}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="flex-1 justify-start text-muted-foreground"
              onClick={() => window.open(href, "_blank", "noopener")}
            >
              <ExternalLink /> {t`Open`}
            </Button>
          </div>
        )}
      </PopoverContent>
    </Popover>
  )
}

export function LinkBubbleMenu({ editor }: { editor: Editor }) {
  const { t } = useLingui()
  const href = useEditorState({
    editor,
    selector: ({ editor: instance }) =>
      (instance.getAttributes("link").href as string) ?? "",
  })

  return (
    <BubbleMenu
      editor={editor}
      pluginKey="linkBubbleMenu"
      options={BOTTOM_OPTIONS}
      shouldShow={showOnCollapsedLink}
      className={cn(panelClass, "max-w-sm")}
    >
      <a
        href={href}
        target="_blank"
        rel="noopener noreferrer"
        className="max-w-56 truncate px-2 text-xs text-primary underline-offset-2 hover:underline"
      >
        {href}
      </a>
      <ToolbarSeparator />
      <ToolbarButton
        label={t`Copy`}
        onClick={() => navigator.clipboard.writeText(href)}
      >
        <Copy />
      </ToolbarButton>
      <ToolbarButton
        label={t`Edit`}
        onClick={() => {
          const next = window.prompt(t`Link address`, href)
          if (next !== null) {
            editor
              .chain()
              .focus()
              .extendMarkRange("link")
              .setLink({ href: next })
              .run()
          }
        }}
      >
        <Pencil />
      </ToolbarButton>
      <ToolbarButton
        label={t`Remove link`}
        onClick={() =>
          editor.chain().focus().extendMarkRange("link").unsetLink().run()
        }
      >
        <Link2Off />
      </ToolbarButton>
    </BubbleMenu>
  )
}

export function ImageBubbleMenu({ editor }: { editor: Editor }) {
  const { t } = useLingui()

  return (
    <BubbleMenu
      editor={editor}
      pluginKey="imageBubbleMenu"
      options={TOP_OPTIONS}
      shouldShow={showOnImage}
      className={panelClass}
    >
      <ToolbarButton
        label={t`Align left`}
        active={editor.isActive("image", { textAlign: "left" })}
        onClick={() => editor.chain().focus().setTextAlign("left").run()}
      >
        <AlignLeft />
      </ToolbarButton>
      <ToolbarButton
        label={t`Align center`}
        onClick={() => editor.chain().focus().setTextAlign("center").run()}
      >
        <AlignCenter />
      </ToolbarButton>
      <ToolbarButton
        label={t`Align right`}
        onClick={() => editor.chain().focus().setTextAlign("right").run()}
      >
        <AlignRight />
      </ToolbarButton>
      <ToolbarSeparator />
      <ToolbarButton
        label={t`Alt text`}
        onClick={() => {
          const alt = window.prompt(
            t`Alt text`,
            (editor.getAttributes("image").alt as string) ?? ""
          )
          if (alt !== null)
            editor.chain().focus().updateAttributes("image", { alt }).run()
        }}
      >
        <Pencil />
      </ToolbarButton>
      <ToolbarButton
        label={t`Full width`}
        onClick={() =>
          editor
            .chain()
            .focus()
            .updateAttributes("image", { width: null })
            .run()
        }
      >
        <Maximize2 />
      </ToolbarButton>
      <ToolbarButton
        label={t`Delete`}
        className="hover:text-destructive"
        onClick={() => editor.chain().focus().deleteSelection().run()}
      >
        <Trash2 />
      </ToolbarButton>
    </BubbleMenu>
  )
}

export function TableBubbleMenu({ editor }: { editor: Editor }) {
  const { t } = useLingui()

  return (
    <BubbleMenu
      editor={editor}
      pluginKey="tableBubbleMenu"
      options={TABLE_OPTIONS}
      shouldShow={showOnTable}
      className={panelClass}
    >
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="sm"
              className="text-muted-foreground"
            />
          }
        >
          <Columns3 /> {t`Column`}
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <DropdownMenuItem
            onClick={() => editor.chain().focus().addColumnBefore().run()}
          >
            {t`Add column before`}
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => editor.chain().focus().addColumnAfter().run()}
          >
            {t`Add column after`}
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => editor.chain().focus().toggleHeaderColumn().run()}
          >
            {t`Header column`}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            variant="destructive"
            onClick={() => editor.chain().focus().deleteColumn().run()}
          >
            {t`Delete column`}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="sm"
              className="text-muted-foreground"
            />
          }
        >
          <Rows3 /> {t`Row`}
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <DropdownMenuItem
            onClick={() => editor.chain().focus().addRowBefore().run()}
          >
            {t`Add row before`}
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => editor.chain().focus().addRowAfter().run()}
          >
            {t`Add row after`}
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => editor.chain().focus().toggleHeaderRow().run()}
          >
            {t`Header row`}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            variant="destructive"
            onClick={() => editor.chain().focus().deleteRow().run()}
          >
            {t`Delete row`}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <ToolbarSeparator />

      <ToolbarButton
        label={t`Merge cells`}
        onClick={() => editor.chain().focus().mergeCells().run()}
      >
        <Merge />
      </ToolbarButton>
      <ToolbarButton
        label={t`Split cell`}
        onClick={() => editor.chain().focus().splitCell().run()}
      >
        <Split />
      </ToolbarButton>
      <ToolbarButton
        label={t`Delete table`}
        className="hover:text-destructive"
        onClick={() => editor.chain().focus().deleteTable().run()}
      >
        <Trash2 />
      </ToolbarButton>
    </BubbleMenu>
  )
}
