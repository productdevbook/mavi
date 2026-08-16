import * as React from "react"
import type { Editor } from "@tiptap/core"
import { DragHandle } from "@tiptap/extension-drag-handle-react"
import type { Node as PMNode } from "@tiptap/pm/model"
import { useLingui } from "@lingui/react/macro"
import {
  Copy,
  CopyPlus,
  GripVertical,
  Heading1,
  Heading2,
  Heading3,
  List,
  ListOrdered,
  ListTodo,
  Pilcrow,
  Quote,
  Trash2,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

type TFn = (strings: TemplateStringsArray, ...values: unknown[]) => string

function getTurnIntoOptions(t: TFn) {
  return [
    {
      label: t`Paragraph`,
      icon: Pilcrow,
      run: (e: Editor) => e.chain().focus().setParagraph().run(),
    },
    {
      label: t`Heading 1`,
      icon: Heading1,
      run: (e: Editor) =>
        e.chain().focus().setNode("heading", { level: 1 }).run(),
    },
    {
      label: t`Heading 2`,
      icon: Heading2,
      run: (e: Editor) =>
        e.chain().focus().setNode("heading", { level: 2 }).run(),
    },
    {
      label: t`Heading 3`,
      icon: Heading3,
      run: (e: Editor) =>
        e.chain().focus().setNode("heading", { level: 3 }).run(),
    },
    {
      label: t`Bullet list`,
      icon: List,
      run: (e: Editor) => e.chain().focus().toggleBulletList().run(),
    },
    {
      label: t`Numbered list`,
      icon: ListOrdered,
      run: (e: Editor) => e.chain().focus().toggleOrderedList().run(),
    },
    {
      label: t`Task list`,
      icon: ListTodo,
      run: (e: Editor) => e.chain().focus().toggleTaskList().run(),
    },
    {
      label: t`Quote`,
      icon: Quote,
      run: (e: Editor) => e.chain().focus().toggleBlockquote().run(),
    },
  ]
}

export const BlockHandle = React.memo(function BlockHandle({
  editor,
}: {
  editor: Editor
}) {
  const { t } = useLingui()
  const TURN_INTO = getTurnIntoOptions(t)

  const current = React.useRef<{ node: PMNode | null; pos: number }>({
    node: null,
    pos: -1,
  })

  // DragHandle re-registers its plugin whenever this identity changes, which
  // tears down every other plugin view (slash menu, bubble menus).
  const onNodeChange = React.useCallback(
    ({ node, pos }: { node: PMNode | null; pos: number }) => {
      current.current = { node, pos }
    },
    []
  )

  const selectCurrent = () => {
    if (current.current.pos < 0) return
    editor.commands.setNodeSelection(current.current.pos)
  }

  const duplicate = () => {
    const { node, pos } = current.current
    if (!node || pos < 0) return
    editor
      .chain()
      .focus()
      .insertContentAt(pos + node.nodeSize, node.toJSON())
      .run()
  }

  const remove = () => {
    const { node, pos } = current.current
    if (!node || pos < 0) return
    editor
      .chain()
      .focus()
      .deleteRange({ from: pos, to: pos + node.nodeSize })
      .run()
  }

  const copyText = async () => {
    if (current.current.node) {
      await navigator.clipboard.writeText(current.current.node.textContent)
    }
  }

  return (
    <DragHandle
      editor={editor}
      onNodeChange={onNodeChange}
      className="mavi-drag-handle"
    >
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label={t`Block menu`}
              onMouseDown={selectCurrent}
              className="cursor-grab text-muted-foreground active:cursor-grabbing"
            />
          }
        >
          <GripVertical />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" side="right" className="w-48">
          <DropdownMenuGroup>
            <DropdownMenuLabel>{t`Block`}</DropdownMenuLabel>
          </DropdownMenuGroup>
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>{t`Turn into`}</DropdownMenuSubTrigger>
            <DropdownMenuSubContent className="w-44">
              {TURN_INTO.map((item) => (
                <DropdownMenuItem
                  key={item.label}
                  onClick={() => {
                    selectCurrent()
                    item.run(editor)
                  }}
                >
                  <item.icon /> {item.label}
                </DropdownMenuItem>
              ))}
            </DropdownMenuSubContent>
          </DropdownMenuSub>
          <DropdownMenuItem onClick={duplicate}>
            <CopyPlus /> {t`Duplicate`}
          </DropdownMenuItem>
          <DropdownMenuItem onClick={copyText}>
            <Copy /> {t`Copy text`}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem variant="destructive" onClick={remove}>
            <Trash2 /> {t`Delete`}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </DragHandle>
  )
})
