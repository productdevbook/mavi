/* eslint-disable react-refresh/only-export-components */
import * as React from "react"
import { Extension, type Editor, type Range } from "@tiptap/core"
import { PluginKey } from "@tiptap/pm/state"
import { ReactRenderer } from "@tiptap/react"
import { t } from "@lingui/core/macro"
import { useLingui } from "@lingui/react/macro"
import Suggestion, {
  type SuggestionKeyDownProps,
  type SuggestionProps,
} from "@tiptap/suggestion"
import {
  AlignCenter,
  AlignLeft,
  AlignRight,
  Braces,
  Calendar,
  ChevronRight,
  Code2,
  Heading1,
  Heading2,
  Heading3,
  Heading4,
  Image as ImageIcon,
  Link2,
  List,
  ListOrdered,
  ListTodo,
  Minus,
  Pilcrow,
  Quote,
  Sigma,
  Table as TableIcon,
  Type,
  Upload,
  Video,
} from "lucide-react"

import { cn } from "@/lib/utils"
import { i18n } from "@/i18n"
import { openEditorDialog } from "@/components/editor/editor-events"

export interface SlashItem {
  title: string
  description: string
  group: string
  keywords: string[]
  icon: React.ComponentType<{ className?: string }>
  command: (props: { editor: Editor; range: Range }) => void
}

function getSlashItems(): SlashItem[] {
  return [
    {
      title: t`Paragraph`,
      description: t`Plain text block`,
      group: t`Basic`,
      keywords: ["text", "paragraph", "p"],
      icon: Pilcrow,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).setParagraph().run(),
    },
    {
      title: t`Heading 1`,
      description: t`Large section heading`,
      group: t`Basic`,
      keywords: ["h1", "heading", "title"],
      icon: Heading1,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).setNode("heading", { level: 1 }).run(),
    },
    {
      title: t`Heading 2`,
      description: t`Medium section heading`,
      group: t`Basic`,
      keywords: ["h2", "heading"],
      icon: Heading2,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).setNode("heading", { level: 2 }).run(),
    },
    {
      title: t`Heading 3`,
      description: t`Small section heading`,
      group: t`Basic`,
      keywords: ["h3", "heading"],
      icon: Heading3,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).setNode("heading", { level: 3 }).run(),
    },
    {
      title: t`Heading 4`,
      description: t`Sub-heading`,
      group: t`Basic`,
      keywords: ["h4", "heading"],
      icon: Heading4,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).setNode("heading", { level: 4 }).run(),
    },
    {
      title: t`Bullet list`,
      description: t`Create an unordered list`,
      group: t`Lists`,
      keywords: ["bullet", "list", "ul"],
      icon: List,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).toggleBulletList().run(),
    },
    {
      title: t`Numbered list`,
      description: t`Create an ordered list`,
      group: t`Lists`,
      keywords: ["ordered", "number", "ol"],
      icon: ListOrdered,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).toggleOrderedList().run(),
    },
    {
      title: t`Task list`,
      description: t`Checkbox to-do list`,
      group: t`Lists`,
      keywords: ["task", "todo", "checkbox"],
      icon: ListTodo,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).toggleTaskList().run(),
    },
    {
      title: t`Quote`,
      description: t`Add a blockquote`,
      group: t`Blocks`,
      keywords: ["quote", "blockquote"],
      icon: Quote,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).toggleBlockquote().run(),
    },
    {
      title: t`Code block`,
      description: t`Syntax-highlighted code`,
      group: t`Blocks`,
      keywords: ["code", "pre"],
      icon: Code2,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).toggleCodeBlock().run(),
    },
    {
      title: t`Collapsible section`,
      description: t`Foldable details block`,
      group: t`Blocks`,
      keywords: ["details", "accordion", "toggle"],
      icon: ChevronRight,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).setDetails().run(),
    },
    {
      title: t`Divider`,
      description: t`Add a horizontal rule`,
      group: t`Blocks`,
      keywords: ["hr", "divider", "line"],
      icon: Minus,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).setHorizontalRule().run(),
    },
    {
      title: t`Table`,
      description: t`Insert a 3×3 table`,
      group: t`Insert`,
      keywords: ["table", "grid"],
      icon: TableIcon,
      command: ({ editor, range }) =>
        editor
          .chain()
          .focus()
          .deleteRange(range)
          .insertTable({ rows: 3, cols: 3, withHeaderRow: true })
          .run(),
    },
    {
      title: t`Upload image`,
      description: t`Pick an image from your computer`,
      group: t`Insert`,
      keywords: ["image", "upload", "photo"],
      icon: Upload,
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).run()
        openEditorDialog("image-upload")
      },
    },
    {
      title: t`Image (URL)`,
      description: t`Add an image from a link`,
      group: t`Insert`,
      keywords: ["image", "url", "link"],
      icon: ImageIcon,
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).run()
        openEditorDialog("image-url")
      },
    },
    {
      title: "YouTube",
      description: t`Embed a video`,
      group: t`Insert`,
      keywords: ["youtube", "video", "embed"],
      icon: Video,
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).run()
        openEditorDialog("youtube")
      },
    },
    {
      title: t`Link`,
      description: t`Add hyperlinked text`,
      group: t`Insert`,
      keywords: ["link", "url"],
      icon: Link2,
      command: ({ editor, range }) => {
        editor.chain().focus().deleteRange(range).run()
        openEditorDialog("link")
      },
    },
    {
      title: t`Today's date`,
      description: t`Insert the current date as text`,
      group: t`Insert`,
      keywords: ["date", "today"],
      icon: Calendar,
      command: ({ editor, range }) =>
        editor
          .chain()
          .focus()
          .deleteRange(range)
          .insertContent(
            new Date().toLocaleDateString(i18n.locale, {
              day: "2-digit",
              month: "long",
              year: "numeric",
            })
          )
          .run(),
    },
    {
      title: t`Inline code`,
      description: t`Mark selection as code`,
      group: t`Format`,
      keywords: ["inline", "code"],
      icon: Braces,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).toggleCode().run(),
    },
    {
      title: t`Highlight`,
      description: t`Highlight text with color`,
      group: t`Format`,
      keywords: ["highlight", "mark"],
      icon: Type,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).toggleHighlight().run(),
    },
    {
      title: t`Superscript`,
      description: t`Raise text above the line`,
      group: t`Format`,
      keywords: ["superscript"],
      icon: Sigma,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).toggleSuperscript().run(),
    },
    {
      title: t`Align left`,
      description: t`Align paragraph to the left`,
      group: t`Alignment`,
      keywords: ["align", "left"],
      icon: AlignLeft,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).setTextAlign("left").run(),
    },
    {
      title: t`Align center`,
      description: t`Center the paragraph`,
      group: t`Alignment`,
      keywords: ["align", "center"],
      icon: AlignCenter,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).setTextAlign("center").run(),
    },
    {
      title: t`Align right`,
      description: t`Align paragraph to the right`,
      group: t`Alignment`,
      keywords: ["align", "right"],
      icon: AlignRight,
      command: ({ editor, range }) =>
        editor.chain().focus().deleteRange(range).setTextAlign("right").run(),
    },
  ]
}

function filterItems(query: string): SlashItem[] {
  const items = getSlashItems()
  const normalized = query.trim().toLowerCase()
  if (!normalized) return items
  return items.filter(
    (item) =>
      item.title.toLowerCase().includes(normalized) ||
      item.keywords.some((keyword) => keyword.includes(normalized))
  )
}

interface SlashListHandle {
  onKeyDown: (props: SuggestionKeyDownProps) => boolean
}

const SlashList = React.forwardRef<
  SlashListHandle,
  SuggestionProps<SlashItem, SlashItem>
>((props, ref) => {
  const { t } = useLingui()
  const [selected, setSelected] = React.useState(0)
  const [seenItems, setSeenItems] = React.useState(props.items)
  const containerRef = React.useRef<HTMLDivElement>(null)

  if (seenItems !== props.items) {
    setSeenItems(props.items)
    setSelected(0)
  }

  React.useEffect(() => {
    containerRef.current
      ?.querySelector('[data-selected="true"]')
      ?.scrollIntoView({ block: "nearest" })
  }, [selected])

  const select = React.useCallback(
    (index: number) => {
      const item = props.items[index]
      if (item) props.command(item)
    },
    [props]
  )

  React.useImperativeHandle(ref, () => ({
    onKeyDown: ({ event }) => {
      if (!props.items.length) return false
      if (event.key === "ArrowUp") {
        setSelected((i) => (i + props.items.length - 1) % props.items.length)
        return true
      }
      if (event.key === "ArrowDown") {
        setSelected((i) => (i + 1) % props.items.length)
        return true
      }
      if (event.key === "Enter" || event.key === "Tab") {
        select(selected)
        return true
      }
      return false
    },
  }))

  if (!props.items.length) {
    return (
      <div className="z-50 w-72 rounded-lg bg-popover p-3 text-sm text-muted-foreground shadow-lg ring-1 ring-foreground/10">
        {t`No results found`}
      </div>
    )
  }

  let cursor = -1

  return (
    <div
      ref={containerRef}
      className="z-50 max-h-80 w-80 overflow-y-auto overscroll-contain rounded-xl bg-popover p-1.5 shadow-lg ring-1 ring-foreground/10"
    >
      {Object.entries(
        props.items.reduce<Record<string, SlashItem[]>>((groups, item) => {
          ;(groups[item.group] ??= []).push(item)
          return groups
        }, {})
      ).map(([group, items]) => (
        <div key={group}>
          <div className="px-2 py-1.5 text-[0.7rem] font-medium tracking-wide text-muted-foreground uppercase">
            {group}
          </div>
          {items.map((item) => {
            cursor += 1
            const index = cursor
            return (
              <button
                key={item.title}
                type="button"
                data-selected={index === selected}
                onMouseEnter={() => setSelected(index)}
                onClick={() => select(index)}
                className={cn(
                  "flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left transition-colors",
                  index === selected ? "bg-muted" : "hover:bg-muted/60"
                )}
              >
                <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-background ring-1 ring-border">
                  <item.icon className="size-4" />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium">
                    {item.title}
                  </span>
                  <span className="block truncate text-xs text-muted-foreground">
                    {item.description}
                  </span>
                </span>
              </button>
            )
          })}
        </div>
      ))}
    </div>
  )
})
SlashList.displayName = "SlashList"

export const slashCommandPluginKey = new PluginKey("slashCommand")

export const SlashCommand = Extension.create({
  name: "slashCommand",

  addProseMirrorPlugins() {
    return [
      Suggestion<SlashItem, SlashItem>({
        editor: this.editor,
        char: "/",
        pluginKey: slashCommandPluginKey,
        allowSpaces: false,
        startOfLine: false,
        placement: "bottom-start",
        allow: ({ state, range }) => {
          const $from = state.doc.resolve(range.from)
          return (
            $from.parent.type.name !== "codeBlock" &&
            $from.parent.type.spec.code !== true
          )
        },
        items: ({ query }) => filterItems(query),
        command: ({ editor, range, props }) => props.command({ editor, range }),
        render: () => {
          let renderer: ReactRenderer<
            SlashListHandle,
            SuggestionProps<SlashItem, SlashItem>
          > | null = null
          let unmount: (() => void) | null = null

          return {
            onStart: (props) => {
              renderer = new ReactRenderer(SlashList, {
                props,
                editor: props.editor,
              })
              unmount = props.mount(renderer.element as HTMLElement)
            },
            onUpdate: (props) => renderer?.updateProps(props),
            onKeyDown: (props) => {
              if (props.event.key === "Escape") {
                unmount?.()
                unmount = null
                return true
              }
              return renderer?.ref?.onKeyDown(props) ?? false
            },
            onExit: () => {
              unmount?.()
              unmount = null
              renderer?.destroy()
              renderer = null
            },
          }
        },
      }),
    ]
  },
})
