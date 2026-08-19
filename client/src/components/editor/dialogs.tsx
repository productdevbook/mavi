import * as React from "react"
import type { Editor } from "@tiptap/core"
import { Plural, useLingui } from "@lingui/react/macro"
import { Check, Copy, Download, Upload } from "lucide-react"
import { toast } from "sonner"

import { cn } from "@/lib/utils"
import { every } from "@/lib/api"
import { ApiRefused } from "@/lib/api"
import { upload } from "@/lib/upload"
import { publicFileUrl } from "@/lib/media"
import { apiMessage } from "@/lib/auth"
import type { File as Media } from "@api"
import { downloadFile, shortcut } from "@/lib/editor-utils"
import { htmlToMarkdown, markdownToHtml } from "@/lib/markdown"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Kbd } from "@/components/ui/kbd"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import {
  onEditorDialog,
  type EditorDialogEvent,
} from "@/components/editor/editor-events"

export function EditorDialogs({ editor }: { editor: Editor }) {
  const { t } = useLingui()
  const [open, setOpen] = React.useState<EditorDialogEvent | null>(null)
  const fileInputRef = React.useRef<HTMLInputElement>(null)

  React.useEffect(
    () =>
      onEditorDialog((name) => {
        if (name === "image-upload") {
          fileInputRef.current?.click()
          return
        }
        setOpen(name)
      }),
    []
  )

  const close = () => setOpen(null)

  const handleFiles = async (files: FileList | null) => {
    if (!files?.length) return
    let uploaded = 0
    for (const file of Array.from(files)) {
      try {
        const media = await upload(file, "public")
        editor
          .chain()
          .focus()
          .setImage({ src: publicFileUrl(media.id), alt: file.name })
          .run()
        uploaded += 1
      } catch (error) {
        toast.error(
          error instanceof ApiRefused
            ? apiMessage(error)
            : t`Could not upload ${file.name}`
        )
      }
    }
    if (uploaded > 0) {
      toast.success(
        <Plural value={uploaded} one="# image added" other="# images added" />
      )
    }
    if (fileInputRef.current) fileInputRef.current.value = ""
  }

  return (
    <>
      <input
        ref={fileInputRef}
        type="file"
        accept="image/png,image/jpeg,image/gif,image/webp"
        multiple
        hidden
        onChange={(event) => handleFiles(event.target.files)}
      />
      <ImageUrlDialog
        editor={editor}
        open={open === "image-url"}
        onClose={close}
      />
      <YoutubeDialog
        editor={editor}
        open={open === "youtube"}
        onClose={close}
      />
      <LinkDialog editor={editor} open={open === "link"} onClose={close} />
      <TableDialog editor={editor} open={open === "table"} onClose={close} />
      <ExportDialog editor={editor} open={open === "export"} onClose={close} />
      <ShortcutsDialog open={open === "shortcuts"} onClose={close} />
    </>
  )
}

interface DialogPartProps {
  editor: Editor
  open: boolean
  onClose: () => void
}

function ImageUrlDialog({ editor, open, onClose }: DialogPartProps) {
  const { t } = useLingui()
  const [src, setSrc] = React.useState("")
  const [alt, setAlt] = React.useState("")
  const [library, setLibrary] = React.useState<Media[] | null>(null)

  React.useEffect(() => {
    if (!open) {
      setLibrary(null)
      return
    }
    if (library) return
    every("media.files.list", { query: { kind: "image" } })
      .then((files) =>
        setLibrary(files.filter((file) => file.visibility === "public"))
      )
      .catch(() => setLibrary([]))
  }, [open, library])

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    if (!src.trim()) return
    editor.chain().focus().setImage({ src: src.trim(), alt: alt.trim() }).run()
    setSrc("")
    setAlt("")
    onClose()
  }

  const pickFromLibrary = (media: Media) => {
    editor
      .chain()
      .focus()
      .setImage({ src: publicFileUrl(media.id), alt: media.name })
      .run()
    onClose()
  }

  return (
    <Dialog open={open} onOpenChange={(value) => !value && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t`Add image`}</DialogTitle>
          <DialogDescription>
            {t`Paste an image address, drag and drop one, or pick from your media library.`}
          </DialogDescription>
        </DialogHeader>
        <Tabs defaultValue="url">
          <TabsList className="w-full">
            <TabsTrigger value="url" className="flex-1">
              {t`URL`}
            </TabsTrigger>
            <TabsTrigger value="library" className="flex-1">
              {t`Media library`}
            </TabsTrigger>
          </TabsList>
          <TabsContent value="library">
            {library === null ? (
              <p className="py-6 text-center text-sm text-muted-foreground">
                {t`Loading…`}
              </p>
            ) : library.length === 0 ? (
              <p className="py-6 text-center text-sm text-muted-foreground">
                {t`No media uploaded yet`}
              </p>
            ) : (
              <div className="grid max-h-72 grid-cols-4 gap-2 overflow-y-auto py-1">
                {library.map((media) => (
                  <button
                    key={media.id}
                    type="button"
                    onClick={() => pickFromLibrary(media)}
                    className="aspect-square overflow-hidden rounded-md border border-border hover:border-primary"
                  >
                    <img
                      src={publicFileUrl(media.id)}
                      alt={media.name}
                      className="size-full object-cover"
                    />
                  </button>
                ))}
              </div>
            )}
          </TabsContent>
          <TabsContent value="url">
            <form onSubmit={submit} className="flex flex-col gap-3">
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="image-src">{t`Image address`}</Label>
                <Input
                  id="image-src"
                  autoFocus
                  value={src}
                  onChange={(event) => setSrc(event.target.value)}
                  placeholder="https://…/image.jpg"
                />
              </div>
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="image-alt">{t`Alt text`}</Label>
                <Input
                  id="image-alt"
                  value={alt}
                  onChange={(event) => setAlt(event.target.value)}
                  placeholder={t`Short description of the image`}
                />
              </div>
              <DialogFooter>
                <Button type="button" variant="outline" onClick={onClose}>
                  {t`Cancel`}
                </Button>
                <Button type="submit">{t`Add`}</Button>
              </DialogFooter>
            </form>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  )
}

function YoutubeDialog({ editor, open, onClose }: DialogPartProps) {
  const { t } = useLingui()
  const [url, setUrl] = React.useState("")

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    if (!url.trim()) return
    editor.commands.setYoutubeVideo({
      src: url.trim(),
      width: 720,
      height: 405,
    })
    setUrl("")
    onClose()
  }

  return (
    <Dialog open={open} onOpenChange={(value) => !value && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t`YouTube video`}</DialogTitle>
          <DialogDescription>{t`Paste the video link.`}</DialogDescription>
        </DialogHeader>
        <form onSubmit={submit} className="flex flex-col gap-3">
          <Input
            autoFocus
            value={url}
            onChange={(event) => setUrl(event.target.value)}
            placeholder="https://www.youtube.com/watch?v=…"
          />
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              {t`Cancel`}
            </Button>
            <Button type="submit">{t`Embed`}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function LinkDialog({ editor, open, onClose }: DialogPartProps) {
  const { t } = useLingui()
  const [href, setHref] = React.useState("")
  const [text, setText] = React.useState("")
  const [newTab, setNewTab] = React.useState(true)

  const [wasOpen, setWasOpen] = React.useState(false)

  if (open !== wasOpen) {
    setWasOpen(open)
    if (open) {
      setHref((editor.getAttributes("link").href as string) ?? "")
      const { from, to } = editor.state.selection
      setText(editor.state.doc.textBetween(from, to, " "))
    }
  }

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    const value = href.trim()
    if (!value) return
    const attrs = {
      href: value,
      target: newTab ? "_blank" : null,
    }
    const { from, to } = editor.state.selection

    if (from === to) {
      editor
        .chain()
        .focus()
        .insertContent({
          type: "text",
          text: text.trim() || value,
          marks: [{ type: "link", attrs }],
        })
        .run()
    } else {
      editor.chain().focus().extendMarkRange("link").setLink(attrs).run()
    }
    onClose()
  }

  return (
    <Dialog open={open} onOpenChange={(value) => !value && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t`Link`}</DialogTitle>
          <DialogDescription>{t`Add a hyperlink to the selected text.`}</DialogDescription>
        </DialogHeader>
        <form onSubmit={submit} className="flex flex-col gap-3">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="link-href">{t`Address`}</Label>
            <Input
              id="link-href"
              autoFocus
              value={href}
              onChange={(event) => setHref(event.target.value)}
              placeholder="https://example.com"
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="link-text">{t`Text`}</Label>
            <Input
              id="link-text"
              value={text}
              onChange={(event) => setText(event.target.value)}
              placeholder={t`Link text`}
            />
          </div>
          <div className="flex items-center gap-2">
            <Switch
              id="link-tab"
              checked={newTab}
              onCheckedChange={setNewTab}
            />
            <Label htmlFor="link-tab">{t`Open in new tab`}</Label>
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              {t`Cancel`}
            </Button>
            <Button type="submit">{t`Apply`}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function TableDialog({ editor, open, onClose }: DialogPartProps) {
  const { t } = useLingui()
  const [hover, setHover] = React.useState({ rows: 3, cols: 3 })
  const [withHeader, setWithHeader] = React.useState(true)

  const insert = (rows: number, cols: number) => {
    editor
      .chain()
      .focus()
      .insertTable({ rows, cols, withHeaderRow: withHeader })
      .run()
    onClose()
  }

  return (
    <Dialog open={open} onOpenChange={(value) => !value && onClose()}>
      <DialogContent className="sm:max-w-xs">
        <DialogHeader>
          <DialogTitle>{t`Add table`}</DialogTitle>
          <DialogDescription>
            {t`${hover.rows} rows × ${hover.cols} columns`}
          </DialogDescription>
        </DialogHeader>
        <div className="grid grid-cols-8 gap-1">
          {Array.from({ length: 64 }, (_, index) => {
            const row = Math.floor(index / 8) + 1
            const col = (index % 8) + 1
            const active = row <= hover.rows && col <= hover.cols
            return (
              <button
                key={index}
                type="button"
                onMouseEnter={() => setHover({ rows: row, cols: col })}
                onClick={() => insert(row, col)}
                className={cn(
                  "size-6 rounded-[4px] border transition-colors",
                  active
                    ? "border-primary bg-primary/20"
                    : "border-border bg-muted/40"
                )}
              />
            )
          })}
        </div>
        <div className="flex items-center gap-2">
          <Switch
            id="table-header"
            checked={withHeader}
            onCheckedChange={setWithHeader}
          />
          <Label htmlFor="table-header">{t`Add header row`}</Label>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function ExportDialog({ editor, open, onClose }: DialogPartProps) {
  const { t } = useLingui()
  const [importValue, setImportValue] = React.useState("")
  const [copied, setCopied] = React.useState<string | null>(null)

  const html = open ? editor.getHTML() : ""
  const json = open ? JSON.stringify(editor.getJSON(), null, 2) : ""
  const markdown = open ? htmlToMarkdown(html) : ""
  const text = open ? editor.getText() : ""

  const copy = async (label: string, value: string) => {
    await navigator.clipboard.writeText(value)
    setCopied(label)
    window.setTimeout(() => setCopied(null), 1500)
  }

  const formats = [
    { id: "html", label: "HTML", value: html, ext: "html", mime: "text/html" },
    {
      id: "markdown",
      label: "Markdown",
      value: markdown,
      ext: "md",
      mime: "text/markdown",
    },
    {
      id: "json",
      label: "JSON",
      value: json,
      ext: "json",
      mime: "application/json",
    },
    { id: "text", label: t`Text`, value: text, ext: "txt", mime: "text/plain" },
  ]

  const importMarkdown = () => {
    if (!importValue.trim()) return
    editor.commands.setContent(markdownToHtml(importValue))
    setImportValue("")
    toast.success(t`Markdown imported`)
    onClose()
  }

  return (
    <Dialog open={open} onOpenChange={(value) => !value && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t`Import / export`}</DialogTitle>
          <DialogDescription>
            {t`Copy your post in different formats, download it, or import Markdown.`}
          </DialogDescription>
        </DialogHeader>
        <Tabs defaultValue="html">
          <TabsList className="w-full">
            {formats.map((format) => (
              <TabsTrigger key={format.id} value={format.id}>
                {format.label}
              </TabsTrigger>
            ))}
            <TabsTrigger value="import">{t`Import`}</TabsTrigger>
          </TabsList>

          {formats.map((format) => (
            <TabsContent key={format.id} value={format.id} className="mt-3">
              <Textarea
                readOnly
                value={format.value}
                className="h-64 resize-none font-mono text-xs"
              />
              <div className="mt-3 flex justify-end gap-2">
                <Button
                  variant="outline"
                  onClick={() => copy(format.id, format.value)}
                >
                  {copied === format.id ? <Check /> : <Copy />}
                  {t`Copy`}
                </Button>
                <Button
                  onClick={() =>
                    downloadFile(
                      `mavi-post.${format.ext}`,
                      format.value,
                      format.mime
                    )
                  }
                >
                  <Download /> {t`Download`}
                </Button>
              </div>
            </TabsContent>
          ))}

          <TabsContent value="import" className="mt-3">
            <Textarea
              value={importValue}
              onChange={(event) => setImportValue(event.target.value)}
              placeholder={t`# Heading\n\nPaste your Markdown here…`}
              className="h-64 resize-none font-mono text-xs"
            />
            <div className="mt-3 flex justify-end">
              <Button onClick={importMarkdown}>
                <Upload /> {t`Replace content`}
              </Button>
            </div>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  )
}

function useShortcutSections(): Array<{
  group: string
  items: Array<[string, string]>
}> {
  const { t } = useLingui()
  return [
    {
      group: t`Formatting`,
      items: [
        [t`Bold`, "Mod+B"],
        [t`Italic`, "Mod+I"],
        [t`Underline`, "Mod+U"],
        [t`Strikethrough`, "Mod+Shift+S"],
        [t`Inline code`, "Mod+E"],
        [t`Link`, "Mod+K"],
        [t`Clear formatting`, "Mod+\\"],
      ],
    },
    {
      group: t`Blocks`,
      items: [
        [t`Heading 1–6`, "Mod+Alt+1…6"],
        [t`Paragraph`, "Mod+Alt+0"],
        [t`Bullet list`, "Mod+Shift+8"],
        [t`Numbered list`, "Mod+Shift+7"],
        [t`Task list`, "Mod+Shift+9"],
        [t`Quote`, "Mod+Shift+B"],
        [t`Code block`, "Mod+Alt+C"],
      ],
    },
    {
      group: t`Editing`,
      items: [
        [t`Undo`, "Mod+Z"],
        [t`Redo`, "Mod+Shift+Z"],
        [t`Select all`, "Mod+A"],
        [t`Find and replace`, "Mod+Shift+F"],
        [t`Command menu`, "/"],
        [t`Mention someone`, "@"],
        [t`Emoji`, ":"],
      ],
    },
    {
      group: t`View`,
      items: [
        [t`Focus mode`, "Mod+Shift+O"],
        [t`Fullscreen`, "Mod+Shift+Enter"],
        [t`Shortcuts`, "Mod+/"],
      ],
    },
  ]
}

function ShortcutsDialog({
  open,
  onClose,
}: {
  open: boolean
  onClose: () => void
}) {
  const { t } = useLingui()
  const SHORTCUTS = useShortcutSections()

  return (
    <Dialog open={open} onOpenChange={(value) => !value && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t`Keyboard shortcuts`}</DialogTitle>
          <DialogDescription>
            {t`The most common shortcuts in the editor.`}
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-6 sm:grid-cols-2">
          {SHORTCUTS.map((section) => (
            <div key={section.group}>
              <p className="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">
                {section.group}
              </p>
              <ul className="flex flex-col gap-1.5">
                {section.items.map(([label, keys]) => (
                  <li
                    key={label}
                    className="flex items-center justify-between gap-4"
                  >
                    <span className="text-sm">{label}</span>
                    <Kbd>{shortcut(keys)}</Kbd>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}
