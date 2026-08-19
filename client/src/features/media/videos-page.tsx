import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Film, Loader2, Trash2, Upload } from "lucide-react"
import { toast } from "sonner"

import { api, every, Refused } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { File as VideoFile } from "@api"
import { Button } from "@/components/ui/button"
import {
  DashboardEmpty,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

export function VideosPage() {
  const { t } = useLingui()

  const [videos, setVideos] = React.useState<VideoFile[] | null>(null)
  const [uploading, setUploading] = React.useState(false)
  const chooser = React.useRef<HTMLInputElement>(null)

  const load = React.useCallback(() => {
    every("files.list")
      .then((files) =>
        setVideos(
          files.filter(
            (file) => file.kind === "video" || file.mime.startsWith("video/")
          )
        )
      )
      .catch((why: unknown) => {
        toast.error(said(why))
        setVideos((held) => held ?? [])
      })
  }, [])

  React.useEffect(load, [load])

  const upload = async (files: FileList | null) => {
    const file = files?.[0]

    if (!file) return

    setUploading(true)

    try {
      const response = await fetch(
        `/api/files?name=${encodeURIComponent(file.name)}`,
        { method: "POST", body: file }
      )

      if (!response.ok) {
        const why = await response.json().catch(() => null)

        throw new Refused(
          response.status,
          String(why?.error?.code ?? "internal"),
          why?.error?.key ?? null,
          why?.error?.named ?? {},
          String(why?.error?.message ?? response.statusText)
        )
      }

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

  const remove = async (video: VideoFile) => {
    try {
      await api("files.remove", { path: { id: video.id } })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`Videos`}
        description={t`What a lesson plays. Nothing here is public: a video is watched by somebody the course lets in.`}
        actions={
          <>
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
          </>
        }
      />

      {videos === null ? (
        <DashboardLoading />
      ) : videos.length === 0 ? (
        <DashboardEmpty
          icon={Film}
          title={t`No videos yet.`}
          description={t`Upload a video to use in a course lesson.`}
          action={
            <Button onClick={() => chooser.current?.click()}>
              <Upload /> {t`Upload`}
            </Button>
          }
        />
      ) : (
        <div className="flex max-w-3xl flex-col divide-y divide-border rounded-xl border border-border">
          {videos.map((video) => (
            <div key={video.id} className="flex items-center gap-3 px-4 py-3">
              <Film className="size-4 shrink-0 text-muted-foreground" />

              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">{video.name}</p>
                <p className="truncate text-xs text-muted-foreground">
                  {new Date(video.created_at).toLocaleString()}
                </p>
              </div>

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
    </div>
  )
}
