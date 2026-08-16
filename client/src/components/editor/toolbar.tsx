import { type Editor, useEditorState } from "@tiptap/react"
import { useLingui } from "@lingui/react/macro"
import {
  AlignCenter,
  AlignJustify,
  AlignLeft,
  AlignRight,
  Baseline,
  Bold,
  Braces,
  ChevronDown,
  ChevronRight,
  Code2,
  Eraser,
  Highlighter,
  Image as ImageIcon,
  IndentDecrease,
  IndentIncrease,
  Italic,
  Link2,
  List,
  ListOrdered,
  ListTodo,
  Minus,
  Quote,
  Redo2,
  Search,
  Strikethrough,
  Subscript as SubscriptIcon,
  Superscript as SuperscriptIcon,
  Table as TableIcon,
  Underline,
  Undo2,
  Video,
} from "lucide-react"

import { cn } from "@/lib/utils"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Button } from "@/components/ui/button"
import { openEditorDialog } from "@/components/editor/editor-events"
import {
  ToolbarButton,
  ToolbarGroup,
  ToolbarSeparator,
} from "@/components/editor/toolbar-button"
import {
  FONT_SIZES,
  LINE_HEIGHTS,
  useFontFamilies,
  useHighlightColors,
  useTextColors,
} from "@/components/editor/palette"

// On a narrow screen the toolbar scrolls sideways, which makes it a clipping
// ancestor about fourteen hundred pixels wide — so a menu anchored inside it is
// placed where there is "room", off the side of the phone. The screen is the
// boundary that matters.
const onScreen = {
  collisionBoundary:
    typeof document === "undefined" ? undefined : document.documentElement,
  collisionPadding: 8,
} as const

type BlockValue =
  | "paragraph"
  | "h1"
  | "h2"
  | "h3"
  | "h4"
  | "h5"
  | "h6"
  | "blockquote"
  | "codeBlock"

function applyBlock(editor: Editor, value: BlockValue) {
  const chain = editor.chain().focus()
  if (value === "paragraph") return chain.setParagraph().run()
  if (value === "blockquote") return chain.toggleBlockquote().run()
  if (value === "codeBlock") return chain.toggleCodeBlock().run()
  return chain
    .setNode("heading", {
      level: Number(value.slice(1)) as 1 | 2 | 3 | 4 | 5 | 6,
    })
    .run()
}

export function Toolbar({ editor }: { editor: Editor }) {
  const { t } = useLingui()

  const BLOCK_TYPES: Array<{ label: string; value: BlockValue }> = [
    { label: t`Paragraph`, value: "paragraph" },
    { label: t`Heading 1`, value: "h1" },
    { label: t`Heading 2`, value: "h2" },
    { label: t`Heading 3`, value: "h3" },
    { label: t`Heading 4`, value: "h4" },
    { label: t`Heading 5`, value: "h5" },
    { label: t`Heading 6`, value: "h6" },
    { label: t`Quote`, value: "blockquote" },
    { label: t`Code block`, value: "codeBlock" },
  ]

  const TEXT_COLORS = useTextColors()
  const HIGHLIGHT_COLORS = useHighlightColors()
  const FONT_FAMILIES = useFontFamilies()

  const state = useEditorState({
    editor,
    selector: ({ editor: instance }) => ({
      canUndo: instance.can().undo(),
      canRedo: instance.can().redo(),
      bold: instance.isActive("bold"),
      italic: instance.isActive("italic"),
      underline: instance.isActive("underline"),
      strike: instance.isActive("strike"),
      code: instance.isActive("code"),
      subscript: instance.isActive("subscript"),
      superscript: instance.isActive("superscript"),
      link: instance.isActive("link"),
      highlight: instance.isActive("highlight"),
      bulletList: instance.isActive("bulletList"),
      orderedList: instance.isActive("orderedList"),
      taskList: instance.isActive("taskList"),
      blockquote: instance.isActive("blockquote"),
      codeBlock: instance.isActive("codeBlock"),
      details: instance.isActive("details"),
      table: instance.isActive("table"),
      alignLeft: instance.isActive({ textAlign: "left" }),
      alignCenter: instance.isActive({ textAlign: "center" }),
      alignRight: instance.isActive({ textAlign: "right" }),
      alignJustify: instance.isActive({ textAlign: "justify" }),
      color: (instance.getAttributes("textStyle").color as string) ?? "",
      fontFamily:
        (instance.getAttributes("textStyle").fontFamily as string) ?? "",
      fontSize: (instance.getAttributes("textStyle").fontSize as string) ?? "",
      inList: instance.isActive("listItem") || instance.isActive("taskItem"),
      block: (() => {
        if (instance.isActive("codeBlock")) return "codeBlock"
        if (instance.isActive("blockquote")) return "blockquote"
        for (const level of [1, 2, 3, 4, 5, 6] as const) {
          if (instance.isActive("heading", { level })) return `h${level}`
        }
        return "paragraph"
      })() as BlockValue,
    }),
  })

  const activeBlock =
    BLOCK_TYPES.find((item) => item.value === state.block) ?? BLOCK_TYPES[0]

  const indent = () => {
    if (editor.isActive("taskItem")) {
      editor.chain().focus().sinkListItem("taskItem").run()
    } else {
      editor.chain().focus().sinkListItem("listItem").run()
    }
  }

  const outdent = () => {
    if (editor.isActive("taskItem")) {
      editor.chain().focus().liftListItem("taskItem").run()
    } else {
      editor.chain().focus().liftListItem("listItem").run()
    }
  }

  return (
    // Thirty-odd tools wrap to five rows on a phone — a fifth of the screen
    // gone before a word is typed. One scrolling row instead, and nothing
    // dropped: everything here is reachable at every width.
    <div className="mavi-scroll-x flex items-center gap-x-1 gap-y-1.5 overflow-x-auto px-3 py-1.5 *:shrink-0 lg:flex-wrap lg:overflow-x-visible">
      <ToolbarGroup>
        <ToolbarButton
          label={t`Undo`}
          keys="Mod+Z"
          disabled={!state.canUndo}
          onClick={() => editor.chain().focus().undo().run()}
        >
          <Undo2 />
        </ToolbarButton>
        <ToolbarButton
          label={t`Redo`}
          keys="Mod+Shift+Z"
          disabled={!state.canRedo}
          onClick={() => editor.chain().focus().redo().run()}
        >
          <Redo2 />
        </ToolbarButton>
      </ToolbarGroup>

      <ToolbarSeparator />

      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="sm"
              className="min-w-28 justify-between font-normal text-muted-foreground"
            />
          }
        >
          {activeBlock.label}
          <ChevronDown className="size-3.5" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-48" {...onScreen}>
          {BLOCK_TYPES.map((item) => (
            <DropdownMenuItem
              key={item.value}
              onClick={() => applyBlock(editor, item.value)}
              className={cn(item.value === state.block && "bg-muted")}
            >
              <span
                className={cn(
                  item.value.startsWith("h") && "font-semibold",
                  item.value === "h1" && "text-lg",
                  item.value === "h2" && "text-base",
                  item.value === "h3" && "text-sm"
                )}
              >
                {item.label}
              </span>
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>

      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="sm"
              className="min-w-24 justify-between font-normal text-muted-foreground"
            />
          }
        >
          <span className="truncate">
            {FONT_FAMILIES.find((f) => f.value === state.fontFamily)?.name ??
              t`Font`}
          </span>
          <ChevronDown className="size-3.5" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-44" {...onScreen}>
          {FONT_FAMILIES.map((font) => (
            <DropdownMenuItem
              key={font.id}
              style={{ fontFamily: font.value || undefined }}
              onClick={() =>
                font.value
                  ? editor.chain().focus().setFontFamily(font.value).run()
                  : editor.chain().focus().unsetFontFamily().run()
              }
            >
              {font.name}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>

      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="sm"
              className="min-w-16 justify-between font-normal text-muted-foreground"
            />
          }
        >
          {state.fontSize || "16px"}
          <ChevronDown className="size-3.5" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-28" {...onScreen}>
          <DropdownMenuItem
            onClick={() => editor.chain().focus().unsetFontSize().run()}
          >
            {t`Default`}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          {FONT_SIZES.map((size) => (
            <DropdownMenuItem
              key={size}
              onClick={() => editor.chain().focus().setFontSize(size).run()}
            >
              {size}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>

      <ToolbarSeparator />

      <ToolbarGroup>
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
          keys="Mod+Shift+S"
          active={state.strike}
          onClick={() => editor.chain().focus().toggleStrike().run()}
        >
          <Strikethrough />
        </ToolbarButton>
        <ToolbarButton
          label={t`Inline code`}
          keys="Mod+E"
          active={state.code}
          onClick={() => editor.chain().focus().toggleCode().run()}
        >
          <Braces />
        </ToolbarButton>
      </ToolbarGroup>

      <ToolbarGroup>
        <Popover>
          <PopoverTrigger
            render={
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Text color`}
                className="relative text-muted-foreground"
              />
            }
          >
            <Baseline />
            <span
              className="absolute inset-x-1.5 bottom-1 h-0.5 rounded-full"
              style={{ backgroundColor: state.color || "currentColor" }}
            />
          </PopoverTrigger>
          <PopoverContent className="w-56" align="start" {...onScreen}>
            <p className="text-xs font-medium text-muted-foreground">
              {t`Text color`}
            </p>
            <div className="grid grid-cols-6 gap-1">
              {TEXT_COLORS.map((swatch) => (
                <button
                  key={swatch.id}
                  type="button"
                  title={swatch.name}
                  aria-label={swatch.name}
                  onClick={() =>
                    swatch.value
                      ? editor.chain().focus().setColor(swatch.value).run()
                      : editor.chain().focus().unsetColor().run()
                  }
                  className={cn(
                    "size-7 rounded-md border border-border transition-transform hover:scale-110",
                    !swatch.value && "bg-background"
                  )}
                  style={{ backgroundColor: swatch.value || undefined }}
                >
                  {!swatch.value && (
                    <Eraser className="mx-auto size-3.5 text-muted-foreground" />
                  )}
                </button>
              ))}
            </div>
            <label className="flex items-center gap-2 text-xs text-muted-foreground">
              {t`Custom`}
              <input
                type="color"
                value={state.color || "#000000"}
                onChange={(event) =>
                  editor.chain().focus().setColor(event.target.value).run()
                }
                className="h-7 w-full cursor-pointer rounded-md border border-border bg-transparent"
              />
            </label>
          </PopoverContent>
        </Popover>

        <Popover>
          <PopoverTrigger
            render={
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Highlight color`}
                data-active={state.highlight || undefined}
                className="text-muted-foreground data-[active]:bg-primary/10 data-[active]:text-primary"
              />
            }
          >
            <Highlighter />
          </PopoverTrigger>
          <PopoverContent className="w-56" align="start" {...onScreen}>
            <p className="text-xs font-medium text-muted-foreground">
              {t`Highlight`}
            </p>
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
                  className="h-7 rounded-md border border-border transition-transform hover:scale-105"
                  style={{ backgroundColor: swatch.value }}
                />
              ))}
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={() => editor.chain().focus().unsetHighlight().run()}
            >
              <Eraser /> {t`Remove highlight`}
            </Button>
          </PopoverContent>
        </Popover>

        <ToolbarButton
          label={t`Subscript`}
          active={state.subscript}
          onClick={() => editor.chain().focus().toggleSubscript().run()}
        >
          <SubscriptIcon />
        </ToolbarButton>
        <ToolbarButton
          label={t`Superscript`}
          active={state.superscript}
          onClick={() => editor.chain().focus().toggleSuperscript().run()}
        >
          <SuperscriptIcon />
        </ToolbarButton>
        <ToolbarButton
          label={t`Clear formatting`}
          onClick={() =>
            editor.chain().focus().unsetAllMarks().clearNodes().run()
          }
        >
          <Eraser />
        </ToolbarButton>
      </ToolbarGroup>

      <ToolbarSeparator />

      <ToolbarGroup>
        <ToolbarButton
          label={t`Align left`}
          keys="Mod+Shift+L"
          active={state.alignLeft}
          onClick={() => editor.chain().focus().setTextAlign("left").run()}
        >
          <AlignLeft />
        </ToolbarButton>
        <ToolbarButton
          label={t`Align center`}
          keys="Mod+Shift+E"
          active={state.alignCenter}
          onClick={() => editor.chain().focus().setTextAlign("center").run()}
        >
          <AlignCenter />
        </ToolbarButton>
        <ToolbarButton
          label={t`Align right`}
          keys="Mod+Shift+R"
          active={state.alignRight}
          onClick={() => editor.chain().focus().setTextAlign("right").run()}
        >
          <AlignRight />
        </ToolbarButton>
        <ToolbarButton
          label={t`Justify`}
          keys="Mod+Shift+J"
          active={state.alignJustify}
          onClick={() => editor.chain().focus().setTextAlign("justify").run()}
        >
          <AlignJustify />
        </ToolbarButton>
      </ToolbarGroup>

      <ToolbarSeparator />

      <ToolbarGroup>
        <ToolbarButton
          label={t`Bullet list`}
          keys="Mod+Shift+8"
          active={state.bulletList}
          onClick={() => editor.chain().focus().toggleBulletList().run()}
        >
          <List />
        </ToolbarButton>
        <ToolbarButton
          label={t`Numbered list`}
          keys="Mod+Shift+7"
          active={state.orderedList}
          onClick={() => editor.chain().focus().toggleOrderedList().run()}
        >
          <ListOrdered />
        </ToolbarButton>
        <ToolbarButton
          label={t`Task list`}
          keys="Mod+Shift+9"
          active={state.taskList}
          onClick={() => editor.chain().focus().toggleTaskList().run()}
        >
          <ListTodo />
        </ToolbarButton>
        <ToolbarButton
          label={t`Decrease indent`}
          keys="Shift+Tab"
          disabled={!state.inList}
          onClick={outdent}
        >
          <IndentDecrease />
        </ToolbarButton>
        <ToolbarButton
          label={t`Increase indent`}
          keys="Tab"
          disabled={!state.inList}
          onClick={indent}
        >
          <IndentIncrease />
        </ToolbarButton>
      </ToolbarGroup>

      <ToolbarSeparator />

      <ToolbarGroup>
        <ToolbarButton
          label={t`Quote`}
          keys="Mod+Shift+B"
          active={state.blockquote}
          onClick={() => editor.chain().focus().toggleBlockquote().run()}
        >
          <Quote />
        </ToolbarButton>
        <ToolbarButton
          label={t`Code block`}
          keys="Mod+Alt+C"
          active={state.codeBlock}
          onClick={() => editor.chain().focus().toggleCodeBlock().run()}
        >
          <Code2 />
        </ToolbarButton>
        <ToolbarButton
          label={t`Collapsible section`}
          active={state.details}
          onClick={() =>
            state.details
              ? editor.chain().focus().unsetDetails().run()
              : editor.chain().focus().setDetails().run()
          }
        >
          <ChevronRight />
        </ToolbarButton>
        <ToolbarButton
          label={t`Divider`}
          onClick={() => editor.chain().focus().setHorizontalRule().run()}
        >
          <Minus />
        </ToolbarButton>
      </ToolbarGroup>

      <ToolbarSeparator />

      <ToolbarGroup>
        <ToolbarButton
          label={t`Link`}
          keys="Mod+K"
          active={state.link}
          onClick={() => openEditorDialog("link")}
        >
          <Link2 />
        </ToolbarButton>
        <ToolbarButton
          label={t`Image`}
          onClick={() => openEditorDialog("image-url")}
        >
          <ImageIcon />
        </ToolbarButton>
        <ToolbarButton
          label={t`YouTube`}
          onClick={() => openEditorDialog("youtube")}
        >
          <Video />
        </ToolbarButton>
        <ToolbarButton
          label={t`Table`}
          active={state.table}
          onClick={() => openEditorDialog("table")}
        >
          <TableIcon />
        </ToolbarButton>
      </ToolbarGroup>

      <ToolbarSeparator />

      <ToolbarGroup>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button
                variant="ghost"
                size="sm"
                className="font-normal text-muted-foreground"
              />
            }
          >
            {t`Line height`}
            <ChevronDown className="size-3.5" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-32" {...onScreen}>
            <DropdownMenuItem
              onClick={() => editor.chain().focus().unsetLineHeight().run()}
            >
              {t`Default`}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            {LINE_HEIGHTS.map((value) => (
              <DropdownMenuItem
                key={value}
                onClick={() =>
                  editor.chain().focus().setLineHeight(value).run()
                }
              >
                {value}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>

        <ToolbarButton
          label={t`Find and replace`}
          keys="Mod+Shift+F"
          onClick={() => openEditorDialog("find-replace")}
        >
          <Search />
        </ToolbarButton>
      </ToolbarGroup>
    </div>
  )
}
