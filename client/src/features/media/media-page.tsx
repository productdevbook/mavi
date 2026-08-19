import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { HardDrive, Loader2, Trash2, Upload } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { File as Media, FileVisibility } from "@api"
import { formatBytes } from "@/lib/editor-utils"
import { usePrivateFileUrl } from "@/lib/media"
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

/** What has been uploaded, with visibility kept explicit at upload time. */
export function MediaPage() {
  const { t } = useLingui()
  const [media, setMedia] = React.useState<Media[] | null>(null)
  const [going, setGoing] = React.useState<Media | null>(null)
  const [uploading, setUploading] = React.useState(false)
  const [visibility, setVisibility] = React.useState<FileVisibility>("private")
  const chooser = React.useRef<HTMLInputElement>(null)

  const load = React.useCallback(() => {
    every("media.files.list", { query: {} })
      .then(setMedia)
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setMedia((held) => held ?? [])
      })
  }, [])

  React.useEffect(() => load(), [load])

  const upload = async (files: FileList | null) => {
    if (!files?.length) return

    setUploading(true)
    let done = 0

    for (const file of Array.from(files)) {
      try {
        await api("media.files.upload", {
          query: { name: file.name, visibility },
          body: file,
        })
        done += 1
      } catch (why) {
        toast.error(apiMessage(why))
      }
    }

    setUploading(false)
    if (chooser.current) chooser.current.value = ""
    if (done > 0) load()
  }

  const remove = async () => {
    if (!going) return

    try {
      await api("media.files.trash", { path: { id: going.id } })
      setMedia((held) => held?.filter((one) => one.id !== going.id) ?? null)
    } catch (why) {
      toast.error(apiMessage(why))
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
            <Select
              value={visibility}
              onValueChange={(value) =>
                setVisibility((value as FileVisibility) ?? "private")
              }
            >
              <SelectTrigger className="w-32" aria-label={t`Visibility`}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="private">{t`Private`}</SelectItem>
                <SelectItem value="public">{t`Public`}</SelectItem>
              </SelectContent>
            </Select>
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
                <MediaThumbnail file={item} alt={item.name} />
              ) : (
                <div className="flex aspect-square items-center justify-center bg-muted">
                  <HardDrive className="size-8 text-muted-foreground" />
                </div>
              )}

              <figcaption className="flex items-center gap-2 px-3 py-2">
                <div className="min-w-0 flex-1">
                  <p className="truncate text-xs font-medium">{item.name}</p>
                  <p className="text-xs text-muted-foreground">
                    {formatBytes(item.bytes)} · {item.visibility}
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

function MediaThumbnail({ file, alt }: { file: Media; alt: string }) {
  const src = usePrivateFileUrl(file)

  return src ? (
    <img
      src={src}
      alt={alt}
      loading="lazy"
      className="aspect-square w-full object-cover"
    />
  ) : (
    <div className="flex aspect-square items-center justify-center bg-muted">
      <Loader2 className="size-8 animate-spin text-muted-foreground" />
    </div>
  )
}
