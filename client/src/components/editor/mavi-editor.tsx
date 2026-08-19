import * as React from "react"
import { EditorContent, useEditor } from "@tiptap/react"
import type { TableOfContentData } from "@tiptap/extension-table-of-contents"
import { useLingui } from "@lingui/react/macro"
import { useNavigate } from "@tanstack/react-router"
import {
  Eye,
  EyeOff,
  FileDown,
  Focus as FocusIcon,
  Keyboard,
  Languages,
  LayoutDashboard,
  ListTree,
  Loader2,
  LogOut,
  Maximize,
  Minimize,
  Monitor,
  Moon,
  MoreHorizontal,
  PanelRightClose,
  PanelRightOpen,
  Save,
  Send,
  Sun,
} from "lucide-react"
import { toast } from "sonner"

import { cn } from "@/lib/utils"
import { shortcut } from "@/lib/editor-utils"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Kbd } from "@/components/ui/kbd"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { nextApi } from "@/lib/server-next"
import { serverNextMessage } from "@/lib/server-next-auth"
import { signOut as authSignOut } from "@/lib/server-next-auth"
import type { Content as Post, Term } from "@api-next"
import { slugify } from "@/lib/editor-utils"
import { contentPublishAt, contentStatus } from "@/lib/server-next-content"
import { useLanguages } from "@/lib/use-languages"
import { useNarrowerThan } from "@/hooks/use-mobile"
import { ModeToggle } from "@/components/mode-toggle"
import { LocaleToggle } from "@/components/locale-toggle"
import { useTheme } from "@/components/theme-provider"
import { locales, setLocale, type Locale } from "@/i18n"
import { BlockHandle } from "@/components/editor/block-handle"
import {
  ImageBubbleMenu,
  LinkBubbleMenu,
  TableBubbleMenu,
  TextBubbleMenu,
} from "@/components/editor/bubble-menus"
import { EditorDialogs } from "@/components/editor/dialogs"
import {
  openEditorDialog,
  onEditorDialog,
} from "@/components/editor/editor-events"
import { buildExtensions } from "@/components/editor/extensions"
import { FindReplacePanel } from "@/components/editor/find-replace"
import { PostSettings } from "@/components/editor/post-settings"
import { StatusBar, type SaveState } from "@/components/editor/status-bar"
import { TocPanel } from "@/components/editor/toc-panel"
import { Toolbar } from "@/components/editor/toolbar"
import { useStatusLabels, type PostMeta } from "@/components/editor/types"

const SCROLL_CONTAINER_ID = "mavi-editor-scroll"
const CHARACTER_LIMIT = null
const BLANK_CONTENT = "<p></p>"

const BLANK_META: PostMeta = {
  title: "",
  slug: "",
  excerpt: "",
  status: "draft",
  publishAt: "",
  language: "",
  categoryIds: [],
  tags: [],
  kind: "post",
  fields: {},
}

function toLocalDateTimeInput(iso: string): string {
  const date = new Date(iso)
  const pad = (value: number) => String(value).padStart(2, "0")
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`
}

function postToMeta(post: Post, terms: Term[]): PostMeta {
  const status = contentStatus(post)
  return {
    title: post.title,
    slug: post.slug,
    excerpt: post.excerpt ?? "",
    status,
    publishAt:
      status === "published" || status === "scheduled"
        ? toLocalDateTimeInput(contentPublishAt(post) ?? post.updated_at)
        : "",
    language: post.language,
    categoryIds: terms
      .filter((term) => term.kind === "category")
      .map((term) => term.id),
    tags: terms.filter((term) => term.kind === "tag").map((term) => term.id),
    kind: post.kind,
    fields: (post.fields as Record<string, unknown> | null) ?? {},
  }
}

function changes(meta: PostMeta, body: string) {
  return {
    title: meta.title,
    slug: meta.slug.trim() || undefined,
    excerpt: meta.excerpt.trim() || null,
    body,
    fields: meta.fields,
  }
}

function termIdsKey(ids: string[]): string {
  return [...new Set(ids)].sort().join(",")
}

export function MaviEditor({
  postId,
  locale: initialLocale,
  translationOf: _translationOf,
  kind,
}: {
  postId: string | null
  locale?: string
  translationOf?: string
  /** Which kind of thing is being written: a page, or a kind the site added. */
  kind?: string
}) {
  const { t } = useLingui()
  const navigate = useNavigate()
  const STATUS_LABELS = useStatusLabels()
  const [meta, setMeta] = React.useState<PostMeta>(
    kind ? { ...BLANK_META, kind } : BLANK_META
  )
  const [currentPostId, setCurrentPostId] = React.useState<string | null>(
    postId
  )
  const { languages, defaultCode, label: languageLabel } = useLanguages()
  const [loadedLocale, setLoadedLocale] = React.useState<string | null>(null)
  const locale = loadedLocale ?? initialLocale ?? defaultCode
  const [translations] = React.useState<Post[]>([])
  const [loading, setLoading] = React.useState(postId !== null)
  const [toc, setToc] = React.useState<TableOfContentData>([])
  const [saveState, setSaveState] = React.useState<SaveState>("idle")
  const [savedAt, setSavedAt] = React.useState<Date | null>(null)
  const tocFits = !useNarrowerThan(1024)
  const settingsFit = !useNarrowerThan(1280)
  const [tocAside, setTocAside] = React.useState(true)
  const [tocDrawer, setTocDrawer] = React.useState(false)
  const [settingsAside, setSettingsAside] = React.useState(true)
  const [settingsDrawer, setSettingsDrawer] = React.useState(false)
  const showToc = tocFits ? tocAside : tocDrawer
  const showSettings = settingsFit ? settingsAside : settingsDrawer
  const setShowToc = tocFits ? setTocAside : setTocDrawer
  const setShowSettings = settingsFit ? setSettingsAside : setSettingsDrawer
  const [focusMode, setFocusMode] = React.useState(false)
  const [preview, setPreview] = React.useState(false)
  const [fullscreen, setFullscreen] = React.useState(false)
  const [findOpen, setFindOpen] = React.useState(false)

  const saveTimer = React.useRef<number | null>(null)

  const extensions = React.useMemo(
    () =>
      buildExtensions({
        characterLimit: CHARACTER_LIMIT,
        onTocUpdate: setToc,
        scrollParent: () =>
          document.getElementById(SCROLL_CONTAINER_ID) ?? window,
      }),
    []
  )

  const editor = useEditor({
    extensions,
    content: BLANK_CONTENT,
    autofocus: "start",
    editorProps: {
      attributes: {
        class: "mavi-prose focus:outline-none",
        spellcheck: "true",
      },
    },
  })

  React.useEffect(() => {
    if (!postId || !editor) return
    let cancelled = false
    Promise.all([
      nextApi("content.read", { path: { id: postId } }),
      nextApi("taxonomy.content_terms.list", { path: { id: postId } }),
    ])
      .then(([post, terms]) => {
        if (cancelled) return

        setMeta(postToMeta(post, terms))
        persistedIntentRef.current = {
          status: contentStatus(post),
          publishAt: contentPublishAt(post) ?? "",
        }
        persistedTermIdsRef.current = termIdsKey(terms.map((term) => term.id))

        editor.commands.setContent(post.body, {
          emitUpdate: false,
          contentType: "markdown",
        })

        if (post.body.trim() !== "" && editor.isEmpty) {
          toast.error(
            t`This post could not be opened, so it has been left untouched.`
          )
          navigate({ to: "/dashboard" })
          return
        }

        setSavedAt(new Date(post.updated_at))
        setLoadedLocale(post.language)
        setLoading(false)
      })
      .catch((why: unknown) => {
        if (cancelled) return
        toast.error(serverNextMessage(why))
        navigate({ to: "/dashboard" })
      })
    return () => {
      cancelled = true
    }
  }, [postId, editor, t, navigate])

  const persistedIntentRef = React.useRef({
    status: "draft" as PostMeta["status"],
    publishAt: "",
  })
  const persistedTermIdsRef = React.useRef("")

  const persist = React.useCallback(
    async (nextMeta: PostMeta, options?: { notify?: boolean }) => {
      if (!editor) return false
      if (!currentPostId && !nextMeta.title.trim()) {
        if (options?.notify) toast.error(t`Give your post a title first`)
        return false
      }
      if (nextMeta.status === "scheduled" && !nextMeta.publishAt) {
        if (options?.notify) {
          toast.error(t`A scheduled post needs a publish date`)
        }
        return false
      }
      setSaveState("saving")
      const written = changes(nextMeta, editor.getMarkdown())
      const wantedTermIds = [...nextMeta.categoryIds, ...nextMeta.tags]
      const termsChanged =
        termIdsKey(wantedTermIds) !== persistedTermIdsRef.current
      const publicationChanged =
        nextMeta.status !== persistedIntentRef.current.status ||
        (nextMeta.status === "scheduled" &&
          nextMeta.publishAt !== persistedIntentRef.current.publishAt)

      try {
        const id = currentPostId
          ? (
              await nextApi("content.update", {
                path: { id: currentPostId },
                body: {
                  ...written,
                  ...(publicationChanged && nextMeta.status === "draft"
                    ? { publication: "draft" as const }
                    : {}),
                },
              })
            ).id
          : (
              await nextApi("content.create", {
                body: {
                  ...written,
                  slug: written.slug || slugify(nextMeta.title),
                  language: locale,
                  kind: nextMeta.kind || "post",
                  publication: "draft",
                },
              })
            ).id

        if (publicationChanged && nextMeta.status !== "draft") {
          if (nextMeta.status === "published") {
            await nextApi("content.publish", { path: { id } })
          } else if (nextMeta.status === "scheduled") {
            await nextApi("content.schedule", {
              path: { id },
              body: { at: new Date(nextMeta.publishAt).toISOString() },
            })
          } else if (nextMeta.status === "archived") {
            await nextApi("content.archive", { path: { id } })
          }
        }

        if (termsChanged) {
          await nextApi("taxonomy.content_terms.replace", {
            path: { id },
            body: { term_ids: wantedTermIds },
          })
          persistedTermIdsRef.current = termIdsKey(wantedTermIds)
        }

        persistedIntentRef.current = {
          status: nextMeta.status,
          publishAt: nextMeta.publishAt,
        }

        setSavedAt(new Date())

        if (!currentPostId) {
          setCurrentPostId(id)
          void navigate({
            to: "/editor/$postId",
            params: { postId: id },
            replace: true,
          })
        }

        setSaveState("saved")
        if (options?.notify) toast.success(t`Draft saved`)
        return true
      } catch (why) {
        setSaveState("idle")
        toast.error(serverNextMessage(why))
        return false
      }
    },
    [editor, currentPostId, navigate, t, locale]
  )

  const scheduleSave = React.useCallback(
    (nextMeta: PostMeta) => {
      if (saveTimer.current) window.clearTimeout(saveTimer.current)
      setSaveState("idle")
      saveTimer.current = window.setTimeout(() => void persist(nextMeta), 900)
    },
    [persist]
  )

  const metaRef = React.useRef(meta)

  React.useEffect(() => {
    metaRef.current = meta
  }, [meta])

  React.useEffect(() => {
    // Only edits made after the post is in place count: extensions stamp ids
    // onto the blank document at startup, which would otherwise autosave the
    // empty editor over the post being loaded.
    if (!editor || loading) return
    const handler = () => scheduleSave(metaRef.current)
    editor.on("update", handler)
    return () => {
      editor.off("update", handler)
    }
  }, [editor, loading, scheduleSave])

  const updateMeta = React.useCallback(
    (patch: Partial<PostMeta>) => {
      setMeta((current) => {
        const next = { ...current, ...patch }
        scheduleSave(next)
        return next
      })
    },
    [scheduleSave]
  )

  // Publishing is the one save whose outcome the writer is told about, so it
  // waits for the server instead of going through the autosave timer. A post
  // whose content type has a required field still empty is refused, and the
  // status has to go back to what it was — otherwise the header reads
  // "Published" over a post the site will never show.
  const publish = React.useCallback(async () => {
    if (saveTimer.current) window.clearTimeout(saveTimer.current)
    const before = metaRef.current.status
    const next: PostMeta = { ...metaRef.current, status: "published" }
    setMeta(next)
    if (await persist(next)) {
      toast.success(t`Post published`, { description: `/blog/${next.slug}` })
    } else {
      setMeta((current) => ({ ...current, status: before }))
    }
  }, [persist, t])

  // Slug tracks the title automatically until the user edits it directly
  // (via post settings) — same "auto until touched" behavior as WordPress's
  // permalink field. Loaded posts already have a real slug, so this starts
  // off for them.
  const autoSlugRef = React.useRef(postId === null)

  const handleSettingsChange = React.useCallback(
    (patch: Partial<PostMeta>) => {
      if (patch.slug !== undefined) autoSlugRef.current = false
      updateMeta(patch)
    },
    [updateMeta]
  )

  // The server decides what a slug looks like, so the address shown while
  // typing is asked for rather than guessed. Debounced: this follows every
  // keystroke in the title.
  const slugTimer = React.useRef<number | undefined>(undefined)
  const requestSlug = React.useCallback(
    (title: string) => {
      if (slugTimer.current) window.clearTimeout(slugTimer.current)
      if (!title.trim()) {
        updateMeta({ slug: "" })
        return
      }
      // Made here rather than asked for: the API makes its own from the title
      // when none is sent, and what this shows is only what somebody would get
      // if they left it alone.
      slugTimer.current = window.setTimeout(() => {
        if (autoSlugRef.current) updateMeta({ slug: slugify(title) })
      }, 400)
    },
    [updateMeta]
  )

  React.useEffect(() => {
    editor?.setEditable(!preview)
  }, [editor, preview])

  React.useEffect(
    () =>
      onEditorDialog((name) => {
        if (name === "find-replace") setFindOpen(true)
      }),
    []
  )

  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const mod = event.metaKey || event.ctrlKey
      if (!mod) return

      if (event.key.toLowerCase() === "s") {
        event.preventDefault()
        void persist(metaRef.current, { notify: true })
      } else if (event.shiftKey && event.key.toLowerCase() === "f") {
        event.preventDefault()
        setFindOpen(true)
      } else if (event.key === "/") {
        event.preventDefault()
        openEditorDialog("shortcuts")
      } else if (event.shiftKey && event.key.toLowerCase() === "o") {
        event.preventDefault()
        setFocusMode((value) => !value)
      } else if (event.shiftKey && event.key === "Enter") {
        event.preventDefault()
        setFullscreen((value) => !value)
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [persist, t])

  React.useEffect(() => {
    if (fullscreen) {
      document.documentElement.requestFullscreen?.().catch(() => undefined)
    } else if (document.fullscreenElement) {
      document.exitFullscreen?.().catch(() => undefined)
    }
  }, [fullscreen])

  if (!editor || loading) {
    return (
      <div className="flex min-h-svh items-center justify-center">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const views: ViewAction[] = [
    {
      key: "toc",
      label: t`Table of contents`,
      icon: <ListTree />,
      active: showToc,
      run: () => setShowToc((value) => !value),
    },
    {
      key: "focus",
      label: t`Focus mode`,
      keys: "Ctrl+Shift+O",
      icon: <FocusIcon />,
      active: focusMode,
      run: () => setFocusMode((value) => !value),
    },
    {
      key: "preview",
      label: preview ? t`Back to editing` : t`Preview`,
      icon: preview ? <EyeOff /> : <Eye />,
      active: preview,
      run: () => setPreview((value) => !value),
    },
    {
      key: "fullscreen",
      label: t`Fullscreen`,
      keys: "Ctrl+Shift+Enter",
      icon: fullscreen ? <Minimize /> : <Maximize />,
      active: fullscreen,
      run: () => setFullscreen((value) => !value),
    },
    {
      key: "export",
      label: t`Import / export`,
      icon: <FileDown />,
      run: () => openEditorDialog("export"),
    },
    {
      key: "shortcuts",
      label: t`Shortcuts`,
      keys: "Ctrl+/",
      icon: <Keyboard />,
      run: () => openEditorDialog("shortcuts"),
    },
  ]

  const otherLanguages =
    locale && languages.length > 1
      ? languages.filter((language) => language.tag !== locale)
      : []

  const goToTranslation = (code: string) => {
    const sibling = translations.find((item) => item.language === code)
    return sibling
      ? navigate({ to: "/editor/$postId", params: { postId: sibling.id } })
      : navigate({
          to: "/editor/new",
          search: { locale: code, translationOf: currentPostId ?? undefined },
        })
  }

  const signOut = () => {
    void authSignOut().finally(() => navigate({ to: "/login" }))
  }

  const settingsPanel = (
    <PostSettings
      meta={meta}
      onChange={handleSettingsChange}
      locale={locale}
      plainText={editor.getText()}
    />
  )

  const tocPanel = (
    <TocPanel
      editor={editor}
      items={toc}
      onNavigate={tocFits ? undefined : () => setShowToc(false)}
    />
  )

  return (
    <div className="flex h-svh flex-col overflow-hidden bg-background">
      <header className="flex items-center gap-1 border-b border-border px-2 py-2 sm:gap-3 sm:px-4">
        <div className="hidden items-center gap-2 lg:flex">
          <span className="flex size-7 items-center justify-center rounded-lg bg-primary text-sm font-bold text-primary-foreground">
            M
          </span>
          <span className="text-sm font-semibold">Mavi CMS</span>
        </div>

        <HeaderButton
          label={t`Dashboard`}
          onClick={() => void navigate({ to: "/dashboard" })}
        >
          <LayoutDashboard />
        </HeaderButton>

        <Separator orientation="vertical" className="hidden h-5 sm:block" />

        {/* The title is the first thing in the article, in letters three times
            this size. Repeating it here is worth a header row only where the
            room is free. */}
        <span className="hidden min-w-0 flex-1 truncate text-sm font-medium text-muted-foreground sm:block">
          {meta.title || t`Untitled post`}
        </span>

        <Badge
          variant={meta.status === "published" ? "default" : "secondary"}
          className="shrink-0"
        >
          {STATUS_LABELS[meta.status]}
        </Badge>

        <div className="flex-1 sm:hidden" />

        {otherLanguages.length > 0 && (
          <DropdownMenu>
            <DropdownMenuTrigger
              render={
                <Button
                  variant="outline"
                  size="sm"
                  className="hidden md:flex"
                />
              }
            >
              <Languages /> {languageLabel(locale ?? "")}
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-56">
              <DropdownMenuGroup>
                <DropdownMenuLabel>{t`Translations`}</DropdownMenuLabel>
                {otherLanguages.map((language) => (
                  <DropdownMenuItem
                    key={language.tag}
                    // A translation can only be started once the post itself
                    // exists — otherwise there is nothing to link it to.
                    disabled={
                      !translations.some(
                        (item) => item.language === language.tag
                      ) && !currentPostId
                    }
                    onClick={() => void goToTranslation(language.tag)}
                  >
                    <span className="flex-1">{language.name}</span>
                    <span className="text-xs text-muted-foreground">
                      {translations.some(
                        (item) => item.language === language.tag
                      )
                        ? t`Edit`
                        : t`Create`}
                    </span>
                  </DropdownMenuItem>
                ))}
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        )}

        <div className="hidden items-center gap-0.5 xl:flex">
          {views.map((view) => (
            <HeaderButton
              key={view.key}
              label={view.label}
              keys={view.keys}
              active={view.active}
              onClick={view.run}
            >
              {view.icon}
            </HeaderButton>
          ))}
          <LocaleToggle />
          <ModeToggle />
          <HeaderButton label={t`Sign out`} onClick={signOut}>
            <LogOut />
          </HeaderButton>
        </div>

        {/* Everything above, for a screen with no room for it. Settings stays
            out of here: on a phone it is the only way to reach the status, the
            address and a content type's own fields. */}
        <EditorMenu
          views={views}
          languages={otherLanguages.map((language) => ({
            code: language.tag,
            name: language.name,
            started: translations.some(
              (item) => item.language === language.tag
            ),
          }))}
          canTranslate={Boolean(currentPostId)}
          onTranslate={(code) => void goToTranslation(code)}
          onSignOut={signOut}
        />

        <HeaderButton
          label={t`Post settings`}
          active={showSettings}
          onClick={() => setShowSettings((value) => !value)}
        >
          {showSettings ? <PanelRightClose /> : <PanelRightOpen />}
        </HeaderButton>

        <Separator orientation="vertical" className="hidden h-5 sm:block" />

        <Button
          variant="outline"
          size="sm"
          aria-label={t`Save`}
          className="px-2 sm:px-3"
          onClick={() => void persist(meta, { notify: true })}
        >
          <Save /> <span className="hidden sm:inline">{t`Save`}</span>
        </Button>
        <Button size="sm" onClick={() => void publish()}>
          <Send /> {t`Publish`}
        </Button>
      </header>

      {!preview && (
        <div className="border-b border-border bg-background/80 backdrop-blur">
          <Toolbar editor={editor} />
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        {tocFits && showToc && (
          <aside className="w-64 shrink-0 border-r border-border">
            <ScrollArea className="h-full">
              <div className="p-3">
                <p className="px-2 pb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">
                  {t`Table of contents`}
                </p>
                {tocPanel}
              </div>
            </ScrollArea>
          </aside>
        )}

        <main
          id={SCROLL_CONTAINER_ID}
          className={cn(
            "relative min-w-0 flex-1 overflow-y-auto",
            focusMode && "mavi-focus-mode"
          )}
        >
          {findOpen && (
            <FindReplacePanel
              editor={editor}
              onClose={() => setFindOpen(false)}
            />
          )}

          <article className="mx-auto w-full max-w-3xl px-4 py-6 sm:px-10 sm:py-10">
            <textarea
              value={meta.title}
              onChange={(event) => {
                const title = event.target.value
                updateMeta({ title })
                if (autoSlugRef.current) requestSlug(title)
              }}
              placeholder={t`Untitled post`}
              rows={1}
              aria-label={t`Post title`}
              readOnly={preview}
              ref={autoSizeTitle}
              onInput={(event) => autoSizeTitle(event.currentTarget)}
              className="mb-4 w-full resize-none bg-transparent text-3xl leading-tight font-bold tracking-tight outline-none placeholder:text-muted-foreground/40 sm:mb-6 sm:text-4xl"
            />
            <EditorContent editor={editor} />
          </article>
        </main>

        {settingsFit && showSettings && !preview && (
          <aside className="w-80 shrink-0 border-l border-border">
            <ScrollArea className="h-full">
              <div className="p-4">
                <p className="pb-4 text-xs font-medium tracking-wide text-muted-foreground uppercase">
                  {t`Post settings`}
                </p>
                {settingsPanel}
              </div>
            </ScrollArea>
          </aside>
        )}
      </div>

      {/* Narrower than the aside needs, the same panels open over the writing
          rather than disappearing from the build. */}
      <Sheet open={!tocFits && showToc} onOpenChange={setShowToc}>
        <SheetContent
          side="left"
          className="gap-0 p-0 data-[side=left]:w-80 data-[side=left]:sm:max-w-80"
        >
          <SheetHeader className="border-b border-border">
            <SheetTitle>{t`Table of contents`}</SheetTitle>
          </SheetHeader>
          <ScrollArea className="min-h-0 flex-1">
            <div className="p-3">{tocPanel}</div>
          </ScrollArea>
        </SheetContent>
      </Sheet>

      <Sheet
        open={!settingsFit && showSettings && !preview}
        onOpenChange={setShowSettings}
      >
        <SheetContent
          side="right"
          className="gap-0 p-0 data-[side=right]:w-full data-[side=right]:sm:max-w-sm"
        >
          <SheetHeader className="border-b border-border">
            <SheetTitle>{t`Post settings`}</SheetTitle>
          </SheetHeader>
          <ScrollArea className="min-h-0 flex-1">
            <div className="p-4">{settingsPanel}</div>
          </ScrollArea>
        </SheetContent>
      </Sheet>

      <StatusBar
        editor={editor}
        saveState={saveState}
        savedAt={savedAt}
        characterLimit={CHARACTER_LIMIT}
      />

      {!preview && (
        <>
          <BlockHandle editor={editor} />
          <TextBubbleMenu editor={editor} />
          <LinkBubbleMenu editor={editor} />
          <ImageBubbleMenu editor={editor} />
          <TableBubbleMenu editor={editor} />
        </>
      )}

      <EditorDialogs editor={editor} />
    </div>
  )
}

function autoSizeTitle(element: HTMLTextAreaElement | null) {
  if (!element) return
  element.style.height = "auto"
  element.style.height = `${element.scrollHeight}px`
}

interface ViewAction {
  key: string
  label: string
  keys?: string
  icon: React.ReactNode
  active?: boolean
  run: () => void
}

function EditorMenu({
  views,
  languages,
  canTranslate,
  onTranslate,
  onSignOut,
}: {
  views: ViewAction[]
  languages: Array<{ code: string; name: string; started: boolean }>
  canTranslate: boolean
  onTranslate: (code: string) => void
  onSignOut: () => void
}) {
  const { t, i18n } = useLingui()
  const { theme, setTheme } = useTheme()

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t`More`}
            className="text-muted-foreground xl:hidden"
          />
        }
      >
        <MoreHorizontal />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        {views.map((view) => (
          <DropdownMenuItem
            key={view.key}
            onClick={view.run}
            className={cn(view.active && "bg-muted")}
          >
            {view.icon} {view.label}
          </DropdownMenuItem>
        ))}

        {languages.length > 0 && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuLabel>{t`Translations`}</DropdownMenuLabel>
            {languages.map((language) => (
              <DropdownMenuItem
                key={language.code}
                disabled={!language.started && !canTranslate}
                onClick={() => onTranslate(language.code)}
              >
                <span className="flex-1">{language.name}</span>
                <span className="text-xs text-muted-foreground">
                  {language.started ? t`Edit` : t`Create`}
                </span>
              </DropdownMenuItem>
            ))}
          </>
        )}

        <DropdownMenuSeparator />
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <Languages /> {t`Language`}
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            {Object.entries(locales).map(([code, label]) => (
              <DropdownMenuItem
                key={code}
                onClick={() => setLocale(code as Locale)}
                className={cn(i18n.locale === code && "bg-muted")}
              >
                {label}
              </DropdownMenuItem>
            ))}
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <Sun /> {t`Theme`}
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            <DropdownMenuItem
              onClick={() => setTheme("light")}
              className={cn(theme === "light" && "bg-muted")}
            >
              <Sun /> {t`Light`}
            </DropdownMenuItem>
            <DropdownMenuItem
              onClick={() => setTheme("dark")}
              className={cn(theme === "dark" && "bg-muted")}
            >
              <Moon /> {t`Dark`}
            </DropdownMenuItem>
            <DropdownMenuItem
              onClick={() => setTheme("system")}
              className={cn(theme === "system" && "bg-muted")}
            >
              <Monitor /> {t`System`}
            </DropdownMenuItem>
          </DropdownMenuSubContent>
        </DropdownMenuSub>

        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={onSignOut}>
          <LogOut /> {t`Sign out`}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function HeaderButton({
  label,
  keys,
  active,
  children,
  onClick,
}: {
  label: string
  keys?: string
  active?: boolean
  children: React.ReactNode
  onClick: () => void
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={label}
            aria-pressed={active}
            data-active={active || undefined}
            onClick={onClick}
            className="text-muted-foreground data-[active]:bg-primary/10 data-[active]:text-primary"
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>
        {label}
        {keys ? <Kbd>{shortcut(keys)}</Kbd> : null}
      </TooltipContent>
    </Tooltip>
  )
}
