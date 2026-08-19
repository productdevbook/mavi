import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { ExternalLink, Loader2 } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Change, ProjectFile } from "@legacy-api"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import {
  DashboardEmpty,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

export function DesignPage() {
  const { t } = useLingui()
  const [changes, setChanges] = React.useState<Change[] | null>(null)
  const [previewing, setPreviewing] = React.useState(false)
  const [publishing, setPublishing] = React.useState(false)

  const load = React.useCallback(() => {
    every("changes.list")
      .then(setChanges)
      .catch((why: unknown) => toast.error(said(why)))
  }, [])

  React.useEffect(load, [load])

  const latest = changes?.[0]

  const preview = () => {
    if (!latest) return
    setPreviewing(true)

    api("changes.build", { path: { id: latest.id } })
      .then(() => {
        toast.success(t`Building preview.`)
        load()
      })
      .catch((why: unknown) => toast.error(said(why)))
      .finally(() => setPreviewing(false))
  }

  const publish = () => {
    if (!latest) return
    setPublishing(true)

    api("changes.publish", { path: { id: latest.id } })
      .then(() => {
        toast.success(t`Publishing.`)
        load()
      })
      .catch((why: unknown) => toast.error(said(why)))
      .finally(() => setPublishing(false))
  }

  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-5">
      <DashboardPageHeader
        title={t`Design`}
        description={t`Edit site templates and static files.`}
        actions={
          <div className="flex items-center gap-2">
            {latest?.look_at && (
              <a
                href={latest.look_at}
                target="_blank"
                rel="noreferrer"
                className="inline-flex h-9 items-center gap-2 rounded-md border border-input bg-background px-4 text-sm font-medium hover:bg-accent"
              >
                {t`Look at it`}
                <ExternalLink className="size-4" />
              </a>
            )}
            {latest && (
              <>
                <Button
                  variant="outline"
                  onClick={preview}
                  disabled={previewing}
                >
                  {previewing && <Loader2 className="size-4 animate-spin" />}
                  {t`Build a preview`}
                </Button>
                <Button onClick={publish} disabled={publishing}>
                  {publishing && <Loader2 className="size-4 animate-spin" />}
                  {t`Publish`}
                </Button>
              </>
            )}
          </div>
        }
      />

      {latest?.went_wrong && (
        <section className="rounded-lg border border-destructive/40 bg-destructive/5 p-4">
          <h2 className="text-sm font-medium text-destructive">{t`The last build did not work`}</h2>
          <pre className="mt-2 max-h-64 overflow-auto text-xs text-muted-foreground">
            {latest.went_wrong}
          </pre>
        </section>
      )}

      <Files changeId={latest?.id} onWritten={load} />
    </div>
  )
}

function Files({
  changeId,
  onWritten,
}: {
  changeId?: string
  onWritten: () => void
}) {
  const { t } = useLingui()

  const [files, setFiles] = React.useState<ProjectFile[] | null>(null)
  const [open, setOpen] = React.useState<string | null>(null)
  const [body, setBody] = React.useState("")
  const [busy, setBusy] = React.useState(false)
  const [making, setMaking] = React.useState(false)
  const [path, setPath] = React.useState("src/")

  const load = React.useCallback(() => {
    api("design.files")
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
      const read = await api("design.read", {
        path: { path: at },
      })

      setBody(read.contents)
    } catch (why) {
      toast.error(said(why))
    }
  }

  const write = async (at: string) => {
    setBusy(true)

    try {
      await api("design.write", {
        path: { path: at },
        body: { change: changeId ?? "", contents: body },
      })
      load()
      onWritten()
      toast.success(t`Saved.`)
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
              onClick={() =>
                void write(path.trim()).then(() => setMaking(false))
              }
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
        <DashboardLoading />
      ) : files.length === 0 ? (
        <DashboardEmpty
          title={t`Nothing written yet.`}
          description={t`A site with no files of its own is served whatever its build makes from nothing.`}
          action={
            <Button
              onClick={() => {
                setMaking(true)
                setOpen(null)
                setBody("")
              }}
            >
              {t`A new file`}
            </Button>
          }
        />
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
