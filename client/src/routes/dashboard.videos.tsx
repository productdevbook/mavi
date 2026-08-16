/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Film, Loader2, Trash2, Upload } from "lucide-react"
import { toast } from "sonner"

import { api, every, Refused } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Video } from "@api"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"

export const Route = createFileRoute("/dashboard/videos")({
  component: VideosRoute,
})

/** How often to look again while something is still being made ready. */
const WATCH_INTERVAL = 5000

/**
 * The videos a site teaches with.
 *
 * A video is uploaded like anything else and then handed to whatever turns it
 * into something a browser can play — which may be this machine doing nothing
 * at all, in which case what was uploaded is what is played.
 */
function VideosRoute() {
  const { t } = useLingui()

  const [videos, setVideos] = React.useState<Video[] | null>(null)
  const [uploading, setUploading] = React.useState(false)
  const chooser = React.useRef<HTMLInputElement>(null)

  const load = React.useCallback(() => {
    every("GET /api/videos")
      .then(setVideos)
      .catch((why: unknown) => {
        toast.error(said(why))
        setVideos((held) => held ?? [])
      })
  }, [])

  React.useEffect(load, [load])

  const working = (videos ?? []).some((one) => one.state === "working")

  React.useEffect(() => {
    if (!working) return

    const timer = window.setInterval(load, WATCH_INTERVAL)

    return () => window.clearInterval(timer)
  }, [working, load])

  const upload = async (files: FileList | null) => {
    const file = files?.[0]

    if (!file) return

    setUploading(true)

    try {
      // The bytes first, as any other upload, and then the video that points
      // at them: what is being watched is a file this site holds.
      const response = await fetch(
        `/api/media?name=${encodeURIComponent(file.name)}`,
        { method: "POST", body: file },
      )

      if (!response.ok) {
        const why = await response.json().catch(() => null)

        throw new Refused(
          response.status,
          String(why?.error?.code ?? "internal"),
          why?.error?.key ?? null,
          why?.error?.named ?? {},
          String(why?.error?.message ?? response.statusText),
        )
      }

      const media = (await response.json()) as { id: string }

      await api("POST /api/videos", {
        body: { media_id: media.id, title: file.name },
      })

      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setUploading(false)

      if (chooser.current) {
        chooser.current.value = ""
      }
    }
  }

  const remove = async (video: Video) => {
    try {
      await api("DELETE /api/videos/{id}", { path: { id: video.id } })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const states: Record<string, string> = {
    waiting: t`Waiting`,
    working: t`Being made ready`,
    ready: t`Ready`,
    failed: t`Did not work`,
  }

  return (
    <>
      <div className="mb-6 flex items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">{t`Videos`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`What a lesson plays. Nothing here is public: a video is watched by somebody the course lets in.`}
          </p>
        </div>

        <Button
          onClick={() => chooser.current?.click()}
          disabled={uploading}
        >
          {uploading ? <Loader2 className="animate-spin" /> : <Upload />}
          {t`Upload`}
        </Button>

        <input
          ref={chooser}
          type="file"
          accept="video/*"
          className="hidden"
          onChange={(event) => void upload(event.target.files)}
        />
      </div>

      {videos === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : videos.length === 0 ? (
        <div className="rounded-xl border border-dashed border-border py-16 text-center">
          <Film className="mx-auto mb-3 size-8 text-muted-foreground" />
          <p className="text-sm text-muted-foreground">{t`No videos yet.`}</p>
        </div>
      ) : (
        <div className="flex max-w-3xl flex-col divide-y divide-border rounded-xl border border-border">
          {videos.map((video) => (
            <div key={video.id} className="flex items-center gap-3 px-4 py-3">
              <Film className="size-4 shrink-0 text-muted-foreground" />

              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">{video.title}</p>
                <p className="truncate text-xs text-muted-foreground">
                  {video.seconds
                    ? t`${Math.round(video.seconds / 60)} minutes`
                    : t`length not known yet`}
                  {video.note ? ` · ${video.note}` : ""}
                </p>
              </div>

              <Badge variant={video.state === "ready" ? "default" : "secondary"}>
                {states[video.state] ?? video.state}
              </Badge>

              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Remove`}
                onClick={() => void remove(video)}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}
    </>
  )
}
