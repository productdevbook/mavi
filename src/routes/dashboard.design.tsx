/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { ExternalLink, Loader2, Palette } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Design } from "../../server/types/mavicms"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"

export const Route = createFileRoute("/dashboard/design")({
  component: DesignRoute,
})

/** How often to look again while something is building. */
const WATCH_INTERVAL = 3000

/**
 * What has been written to the design, and whether it goes live.
 *
 * The changes are on a draft and none of it is the site until somebody presses
 * the button — which is the whole point of the screen. Publishing asks for the
 * Publish permission rather than the Design one, so an account that may ask for
 * changes need not be the one that agrees to them.
 */
function DesignRoute() {
  const { t } = useLingui()
  const [design, setDesign] = React.useState<Design | null>(null)
  const [previewing, setPreviewing] = React.useState(false)
  const [publishing, setPublishing] = React.useState(false)

  const load = React.useCallback(() => {
    api("GET /api/design")
      .then(setDesign)
      .catch((why: unknown) => toast.error(said(why)))
  }, [])

  React.useEffect(load, [load])

  // Only while something is running, and stopped as soon as it is not: a
  // screen left open overnight should not be asking every three seconds.
  const running = design?.building != null

  React.useEffect(() => {
    if (!running) return

    const timer = window.setInterval(load, WATCH_INTERVAL)

    return () => window.clearInterval(timer)
  }, [running, load])

  const preview = () => {
    setPreviewing(true)

    api("POST /api/design/previews")
      .then(() => {
        toast.success(t`Building. The address appears when it is done.`)
        load()
      })
      .catch((why: unknown) => toast.error(said(why)))
      .finally(() => setPreviewing(false))
  }

  const publish = () => {
    setPublishing(true)

    api("POST /api/design/publishes")
      .then(() => {
        toast.success(t`Publishing.`)
        load()
      })
      .catch((why: unknown) => toast.error(said(why)))
      .finally(() => setPublishing(false))
  }

  if (!design) {
    return (
      <div className="flex items-center justify-center py-24">
        <Loader2 className="text-muted-foreground size-5 animate-spin" />
      </div>
    )
  }

  const nothing = design.changed.length === 0
  const failed =
    design.preview?.state === "failed" ? design.preview : null

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-8 py-6">
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex flex-col gap-1">
          <h1 className="flex items-center gap-2 text-xl font-semibold">
            <Palette className="size-5" />
            {t`Design`}
          </h1>
          <p className="text-muted-foreground text-sm">
            {nothing
              ? t`Nothing is waiting. What is published and what is being worked on are the same.`
              : t`${design.changed.length} files changed. None of it is live yet.`}
          </p>
        </div>

        <div className="flex items-center gap-2">
          {design.preview_at && (
            <a
              href={design.preview_at}
              target="_blank"
              rel="noreferrer"
              className="border-input bg-background hover:bg-accent inline-flex h-9 items-center gap-2 rounded-md border px-4 text-sm font-medium"
            >
              {t`Look at it`}
              <ExternalLink className="size-4" />
            </a>
          )}
          <Button
            variant="outline"
            onClick={preview}
            disabled={previewing || nothing || running}
          >
            {(previewing || running) && (
              <Loader2 className="size-4 animate-spin" />
            )}
            {running ? t`Building…` : t`Build a preview`}
          </Button>
          <Button onClick={publish} disabled={publishing || nothing || running}>
            {publishing && <Loader2 className="size-4 animate-spin" />}
            {t`Publish`}
          </Button>
        </div>
      </header>

      {failed?.log && (
        <section className="border-destructive/40 bg-destructive/5 rounded-lg border p-4">
          <h2 className="text-destructive text-sm font-medium">{t`The last preview did not build`}</h2>
          <pre className="text-muted-foreground mt-2 max-h-64 overflow-auto text-xs">
            {failed.log.split("\n").slice(-40).join("\n")}
          </pre>
        </section>
      )}

      {!nothing && (
        <section className="flex flex-col gap-3">
          <h2 className="text-sm font-medium">{t`What changed`}</h2>
          <ul className="flex flex-col gap-1 text-sm">
            {design.changed.map((change) => (
              <li
                key={change.path}
                className="flex items-center gap-2 font-mono text-xs"
              >
                <span className="text-muted-foreground w-16">{change.kind}</span>
                <span>{change.path}</span>
              </li>
            ))}
          </ul>
        </section>
      )}

      <Files onWritten={load} />

      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-medium">{t`What is live`}</h2>
        {design.live ? (
          <p className="text-muted-foreground text-sm">
            {t`Published ${new Date(design.live.created_at).toLocaleString()}, from ${design.live.branch}, in ${design.live.seconds ?? 0} seconds.`}
          </p>
        ) : (
          <p className="text-muted-foreground text-sm">
            {t`Nothing has been published yet.`}
          </p>
        )}
      </section>
    </div>
  )
}

/**
 * The files a site's design is made of.
 *
 * Only `src/` and `public/`: what decides how a site is built is not a thing a
 * site edits, and the API refuses it as well. Writing goes to the draft —
 * nothing here changes what is being served until somebody publishes.
 */
function Files({ onWritten }: { onWritten: () => void }) {
  const { t } = useLingui()

  const [files, setFiles] = React.useState<
    { path: string; branch: string; updated_at: string }[] | null
  >(null)
  const [open, setOpen] = React.useState<string | null>(null)
  const [body, setBody] = React.useState("")
  const [busy, setBusy] = React.useState(false)
  const [making, setMaking] = React.useState(false)
  const [path, setPath] = React.useState("src/")

  const load = React.useCallback(() => {
    api("GET /api/design/files", { query: { branch: "draft" } })
      .then(setFiles)
      .catch((why: unknown) => {
        toast.error(said(why))
        setFiles([])
      })
  }, [])

  React.useEffect(load, [load])

  const look = async (at: string) => {
    setOpen(at)
    setBody("")

    try {
      const read = await api("GET /api/design/file", {
        query: { path: at, branch: "draft" },
      })

      setBody(read.body)
    } catch (why) {
      toast.error(said(why))
    }
  }

  const write = async (at: string) => {
    setBusy(true)

    try {
      await api("PUT /api/design/files", {
        body: { path: at, body, branch: "draft" },
      })
      load()
      onWritten()
      toast.success(t`Saved to the draft.`)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium">{t`The files`}</h2>
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            setMaking(true)
            setOpen(null)
            setBody("")
          }}
        >
          {t`A new file`}
        </Button>
      </div>

      {making && (
        <div className="flex flex-col gap-2 rounded-xl border border-border p-3">
          <input
            className="h-9 rounded-md border border-input bg-transparent px-3 font-mono text-sm"
            value={path}
            onChange={(event) => setPath(event.target.value)}
            placeholder="src/pages/index.astro"
          />
          <Textarea
            rows={12}
            className="font-mono text-xs"
            value={body}
            onChange={(event) => setBody(event.target.value)}
          />
          <div className="flex gap-2">
            <Button
              size="sm"
              disabled={!path.trim() || busy}
              onClick={() => void write(path.trim()).then(() => setMaking(false))}
            >
              {busy && <Loader2 className="animate-spin" />}
              {t`Save`}
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setMaking(false)}>
              {t`Cancel`}
            </Button>
          </div>
        </div>
      )}

      {files === null ? (
        <Loader2 className="size-5 animate-spin text-muted-foreground" />
      ) : files.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          {t`Nothing written yet. A site with no files of its own is served whatever its build makes from nothing.`}
        </p>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {files.map((file) => (
            <div key={file.path} className="px-3 py-2">
              <button
                type="button"
                className="w-full truncate text-left font-mono text-xs hover:underline"
                onClick={() => void look(file.path)}
              >
                {file.path}
              </button>

              {open === file.path && (
                <div className="mt-2 flex flex-col gap-2">
                  <Textarea
                    rows={16}
                    className="font-mono text-xs"
                    value={body}
                    onChange={(event) => setBody(event.target.value)}
                  />
                  <div className="flex gap-2">
                    <Button
                      size="sm"
                      disabled={busy}
                      onClick={() => void write(file.path)}
                    >
                      {busy && <Loader2 className="animate-spin" />}
                      {t`Save`}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setOpen(null)}
                    >
                      {t`Close`}
                    </Button>
                  </div>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </section>
  )
}
