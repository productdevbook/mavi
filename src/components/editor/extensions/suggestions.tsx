/* eslint-disable react-refresh/only-export-components */
import * as React from "react"
import type { EmojiItem } from "@tiptap/extension-emoji"
import type { MentionNodeAttrs } from "@tiptap/extension-mention"
import { ReactRenderer } from "@tiptap/react"
import { useLingui } from "@lingui/react/macro"
import type {
  SuggestionKeyDownProps,
  SuggestionOptions,
  SuggestionProps,
} from "@tiptap/suggestion"

import { cn } from "@/lib/utils"
import { i18n } from "@/i18n"

interface ListHandle {
  onKeyDown: (props: SuggestionKeyDownProps) => boolean
}

function useListNavigation<I, S>(
  props: SuggestionProps<I, S>,
  ref: React.Ref<ListHandle>,
  toSelected: (item: I) => S
) {
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
      if (item) props.command(toSelected(item))
    },
    [props, toSelected]
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

  return { selected, setSelected, select, containerRef }
}

const popupClass =
  "z-50 max-h-72 w-64 overflow-y-auto overscroll-contain rounded-xl bg-popover p-1.5 shadow-lg ring-1 ring-foreground/10"

const rowClass = (active: boolean) =>
  cn(
    "flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm transition-colors",
    active ? "bg-muted" : "hover:bg-muted/60"
  )

export type MentionRole =
  | "editor"
  | "writer"
  | "chiefEditor"
  | "designer"
  | "seo"
  | "developer"
  | "reporter"

export interface MentionUser {
  id: string
  label: string
  role: MentionRole
  initials: string
}

export const MENTION_USERS: MentionUser[] = [
  { id: "1", label: "Ayşe Yılmaz", role: "editor", initials: "AY" },
  { id: "2", label: "Mehmet Kaya", role: "writer", initials: "MK" },
  { id: "3", label: "Zeynep Demir", role: "chiefEditor", initials: "ZD" },
  { id: "4", label: "Can Öztürk", role: "designer", initials: "CÖ" },
  { id: "5", label: "Elif Şahin", role: "seo", initials: "EŞ" },
  { id: "6", label: "Burak Aydın", role: "developer", initials: "BA" },
  { id: "7", label: "Deniz Arslan", role: "reporter", initials: "DA" },
]

type TFn = (strings: TemplateStringsArray, ...values: unknown[]) => string

function getRoleLabels(t: TFn): Record<MentionRole, string> {
  return {
    editor: t`Editor`,
    writer: t`Writer`,
    chiefEditor: t`Chief editor`,
    designer: t`Designer`,
    seo: t`SEO specialist`,
    developer: t`Developer`,
    reporter: t`Reporter`,
  }
}

const MentionList = React.forwardRef<
  ListHandle,
  SuggestionProps<MentionUser, MentionNodeAttrs>
>((props, ref) => {
  const { t } = useLingui()
  const roleLabels = getRoleLabels(t)

  const toSelected = React.useCallback(
    (item: MentionUser) => ({ id: item.id, label: item.label }),
    []
  )
  const { selected, setSelected, select, containerRef } = useListNavigation(
    props,
    ref,
    toSelected
  )

  if (!props.items.length) {
    return (
      <div className="z-50 w-64 rounded-lg bg-popover p-3 text-sm text-muted-foreground shadow-lg ring-1 ring-foreground/10">
        {t`No people found`}
      </div>
    )
  }

  return (
    <div ref={containerRef} className={popupClass}>
      {props.items.map((item, index) => (
        <button
          key={item.id}
          type="button"
          data-selected={index === selected}
          onMouseEnter={() => setSelected(index)}
          onClick={() => select(index)}
          className={rowClass(index === selected)}
        >
          <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-primary/10 text-[0.7rem] font-semibold text-primary">
            {item.initials}
          </span>
          <span className="min-w-0 flex-1">
            <span className="block truncate font-medium">{item.label}</span>
            <span className="block truncate text-xs text-muted-foreground">
              {roleLabels[item.role]}
            </span>
          </span>
        </button>
      ))}
    </div>
  )
})
MentionList.displayName = "MentionList"

const EmojiList = React.forwardRef<
  ListHandle,
  SuggestionProps<EmojiItem, { name: string }>
>((props, ref) => {
  const { t } = useLingui()
  const toSelected = React.useCallback(
    (item: EmojiItem) => ({ name: item.name }),
    []
  )
  const { selected, setSelected, select, containerRef } = useListNavigation(
    props,
    ref,
    toSelected
  )

  if (!props.items.length) {
    return (
      <div className="z-50 w-64 rounded-lg bg-popover p-3 text-sm text-muted-foreground shadow-lg ring-1 ring-foreground/10">
        {t`No emoji found`}
      </div>
    )
  }

  return (
    <div ref={containerRef} className={popupClass}>
      {props.items.map((item, index) => (
        <button
          key={item.name}
          type="button"
          data-selected={index === selected}
          onMouseEnter={() => setSelected(index)}
          onClick={() => select(index)}
          className={rowClass(index === selected)}
        >
          <span className="w-6 text-center text-base leading-none">
            {item.emoji ?? (
              <img src={item.fallbackImage} alt={item.name} className="size-4" />
            )}
          </span>
          <span className="truncate text-muted-foreground">
            :{item.shortcodes[0]}:
          </span>
        </button>
      ))}
    </div>
  )
})
EmojiList.displayName = "EmojiList"

function createRenderer<I, S>(
  Component: React.ComponentType<SuggestionProps<I, S>>
): NonNullable<SuggestionOptions<I, S>["render"]> {
  return () => {
    let renderer: ReactRenderer<ListHandle, SuggestionProps<I, S>> | null = null
    let unmount: (() => void) | null = null

    return {
      onStart: (props) => {
        renderer = new ReactRenderer(Component, {
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
  }
}

export const mentionSuggestion: Omit<
  SuggestionOptions<MentionUser, MentionNodeAttrs>,
  "editor"
> = {
  char: "@",
  placement: "bottom-start",
  items: ({ query }) =>
    MENTION_USERS.filter((user) =>
      user.label.toLocaleLowerCase(i18n.locale).includes(query.toLocaleLowerCase(i18n.locale))
    ).slice(0, 8),
  render: createRenderer(MentionList),
}

export const emojiSuggestion: Partial<
  Omit<SuggestionOptions<EmojiItem, { name: string }>, "editor">
> = {
  items: ({ editor, query }) =>
    editor.storage.emoji.emojis
      .filter(
        (emoji: EmojiItem) =>
          emoji.shortcodes.some((shortcode) =>
            shortcode.startsWith(query.toLowerCase())
          ) || emoji.tags.some((tag) => tag.startsWith(query.toLowerCase()))
      )
      .slice(0, 12),
  render: createRenderer(EmojiList),
}
