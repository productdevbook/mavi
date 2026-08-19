import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { HardDrive, Loader2, Trash2, Upload } from "lucide-react"
import { toast } from "sonner"

import { api, every, Refused } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { File as Media } from "@api"
import { formatBytes } from "@/lib/editor-utils"
import { Button } from "@/components/ui/button"
import {
  DashboardEmpty,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"
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

/** Where an uploaded file is served from. */
const at = (id: string) => `/uploads/${id}`

/**
 * What has been uploaded.
 *
 * An upload is the bytes themselves rather than JSON, so it is the one call
 * the panel makes without the typed client — and what kind of file it is comes
 * from those bytes at the far end, not from the name beside them.
 */
export function MediaPage() {
  const { t } = useLingui()
  const [media, setMedia] = React.useState<Media[] | null>(null)
  const [going, setGoing] = React.useState<Media | null>(null)
  const [uploading, setUploading] = React.useState(false)
  const chooser = React.useRef<HTMLInputElement>(null)

  const load = React.useCallback(() => {
    every("GET /api/files")
      .then(setMedia)
      .catch((why: unknown) => {
        toast.error(said(why))
        setMedia((held) => held ?? [])
      })
  }, [])

  React.useEffect(() => load(), [load])

  const upload = async (files: FileList | null) => {
    if (!files?.length) return

    setUploading(true)
    let done = 0

    try {
      for (const file of Array.from(files)) {
        // The bytes as they are, with the name alongside: what kind of file it
        // is comes from the bytes at the far end rather than from the name.
        const response = await fetch(
          `/api/files?name=${encodeURIComponent(file.name)}`,
          { method: "POST", body: file }
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
                String(why?.error?.message ?? response.statusText)
              )
            )
          )
        }
      }
    } catch (why) {
      toast.error(said(why))
    } finally {
      setUploading(false)
    }

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
      await api("DELETE /api/files/{id}", { path: { id: going.id } })
      setMedia((held) => held?.filter((one) => one.id !== going.id) ?? null)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setGoing(null)
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`Media library`}
        description={t`Upload and manage the files used by this site.`}
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
              accept="image/png,image/jpeg,image/gif,image/webp"
              multiple
              className="hidden"
              onChange={(event) => void upload(event.target.files)}
            />
          </>
        }
      />

      {media === null ? (
        <DashboardLoading />
      ) : media.length === 0 ? (
        <DashboardEmpty
          icon={HardDrive}
          title={t`Nothing uploaded yet.`}
          description={t`Upload an image or file to start building this site's library.`}
          action={
            <Button onClick={() => chooser.current?.click()}>
              <Upload />
              {t`Upload`}
            </Button>
          }
        />
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
                  alt={item.name}
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
                  <p className="truncate text-xs font-medium">{item.name}</p>
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
    </div>
  )
}
