/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { HardDrive, Loader2, Trash2, Upload } from "lucide-react"
import { toast } from "sonner"

import { api, every, Refused } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Media, Settings } from "@api"
import { formatBytes } from "@/lib/editor-utils"
import { Button } from "@/components/ui/button"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"

export const Route = createFileRoute("/dashboard/media")({
  component: MediaRoute,
})

/** Where an uploaded file is served from. */
const at = (id: string) => `/uploads/${id}`

/**
 * What has been uploaded.
 *
 * An upload is the bytes themselves rather than JSON, so it is the one call
 * the panel makes without the typed client — and what kind of file it is comes
 * from those bytes at the far end, not from the name beside them.
 */
function MediaRoute() {
  const { t } = useLingui()
  const [media, setMedia] = React.useState<Media[] | null>(null)
  const [going, setGoing] = React.useState<Media | null>(null)
  const [uploading, setUploading] = React.useState(false)
  const [room, setRoom] = React.useState<Settings | null>(null)
  const chooser = React.useRef<HTMLInputElement>(null)

  const load = React.useCallback(() => {
    every("GET /api/media")
      .then(setMedia)
      .catch((why: unknown) => {
        toast.error(said(why))
        setMedia((held) => held ?? [])
      })

    api("GET /api/site")
      .then(setRoom)
      .catch((why: unknown) => {
        toast.error(said(why))
        setRoom(null)
      })
  }, [])

  React.useEffect(() => load(), [load])

  const upload = async (files: FileList | null) => {
    if (!files?.length) return

    setUploading(true)
    let done = 0

    for (const file of Array.from(files)) {
      // The bytes as they are, with the name alongside: what kind of file it
      // is comes from the bytes at the far end rather than from the name.
      const response = await fetch(
        `/api/media?name=${encodeURIComponent(file.name)}`,
        { method: "POST", body: file },
      )

      if (response.ok) {
        done += 1
      } else {
        const why = await response.json().catch(() => null)

        toast.error(
          said(
            new Refused(
              response.status,
              String(why?.error?.code ?? "internal"),
              why?.error?.key ?? null,
              why?.error?.named ?? {},
              String(why?.error?.message ?? response.statusText),
            ),
          ),
        )
      }
    }

    setUploading(false)

    if (chooser.current) {
      chooser.current.value = ""
    }

    if (done > 0) {
      load()
    }
  }

  const remove = async () => {
    if (!going) return

    try {
      await api("DELETE /api/media/{id}", { path: { id: going.id } })
      setMedia((held) => held?.filter((one) => one.id !== going.id) ?? null)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setGoing(null)
    }
  }

  return (
    <>
      <div className="mb-6 flex items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">{t`Media library`}</h1>
          {room && (
            <p className="text-sm text-muted-foreground">
              {room.storage_limit_bytes
                ? t`${formatBytes(room.storage_used_bytes)} of ${formatBytes(room.storage_limit_bytes)} used`
                : t`${formatBytes(room.storage_used_bytes)} used`}
            </p>
          )}
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
          accept="image/png,image/jpeg,image/gif,image/webp"
          multiple
          className="hidden"
          onChange={(event) => void upload(event.target.files)}
        />
      </div>

      {media === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : media.length === 0 ? (
        <div className="rounded-xl border border-dashed border-border py-16 text-center">
          <HardDrive className="mx-auto mb-3 size-8 text-muted-foreground" />
          <p className="text-sm text-muted-foreground">{t`Nothing uploaded yet.`}</p>
        </div>
      ) : (
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
          {media.map((item) => (
            <figure
              key={item.id}
              className="overflow-hidden rounded-xl border border-border"
            >
              {item.mime.startsWith("image/") ? (
                <img
                  src={at(item.id)}
                  alt={item.original_name}
                  loading="lazy"
                  className="aspect-square w-full object-cover"
                />
              ) : (
                <div className="flex aspect-square items-center justify-center bg-muted">
                  <HardDrive className="size-8 text-muted-foreground" />
                </div>
              )}

              <figcaption className="flex items-center gap-2 px-3 py-2">
                <div className="min-w-0 flex-1">
                  <p className="truncate text-xs font-medium">
                    {item.original_name}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {formatBytes(item.bytes)}
                  </p>
                </div>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t`Delete`}
                  onClick={() => setGoing(item)}
                >
                  <Trash2 />
                </Button>
              </figcaption>
            </figure>
          ))}
        </div>
      )}

      <AlertDialog
        open={going !== null}
        onOpenChange={(open) => !open && setGoing(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Delete this file?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`Anything already using it will stop showing it.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void remove()}>
              {t`Delete`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
