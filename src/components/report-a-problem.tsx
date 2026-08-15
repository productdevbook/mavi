import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { ImagePlus, Loader2, MessageSquareWarning, X } from "lucide-react"
import { toast } from "sonner"

import { api, every, Refused } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { Badge } from "@/components/ui/badge"
import type { Report } from "../../server/types/mavicms"
import { whatWentWrong } from "@/lib/what-went-wrong"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

/** Four megabytes, which is the server's answer too. */
const MOST = 4 * 1024 * 1024

/**
 * Saying something is wrong, or missing.
 *
 * Two lines are asked for and everything else is gathered: which browser,
 * which screen, and whatever has already gone wrong in it. Somebody reporting
 * a broken screen should not also have to know their browser version, and the
 * errors the panel already saw are the half of the story they cannot see.
 *
 * A picture is optional and is the most useful thing anybody sends. Paste one
 * — a screenshot goes on the clipboard, and asking somebody to save a file
 * first is how a report stops being written.
 */
type ReportKind = "broken" | "missing" | "wanted"

export function ReportAProblem({ trigger }: { trigger?: React.ReactNode }) {
  const { t } = useLingui()
  const [open, setOpen] = React.useState(false)
  const [kind, setKind] = React.useState<ReportKind>("broken")
  const [title, setTitle] = React.useState("")
  const [detail, setDetail] = React.useState("")
  const [picture, setPicture] = React.useState<string | null>(null)
  const [pictureFile, setPictureFile] = React.useState<File | null>(null)
  const [sending, setSending] = React.useState(false)
  const [already, setAlready] = React.useState<Report[] | null>(null)

  // What was said before, and what came back. Somebody who has already
  // reported something had no way of knowing it was read.
  React.useEffect(() => {
    if (!open) return

    every("GET /api/reports")
      .then(setAlready)
      .catch((why: unknown) => {
        toast.error(said(why))
        setAlready([])
      })
  }, [open])

  const label: Record<ReportKind, string> = {
    broken: t`Something is broken`,
    missing: t`It cannot do something I need`,
    wanted: t`I would like something`,
  }

  const take = (file: File | null | undefined) => {
    if (!file) return
    if (!file.type.startsWith("image/")) {
      toast.error(t`That is not a picture`)
      return
    }
    if (file.size > MOST) {
      toast.error(t`That picture is larger than four megabytes`)
      return
    }
    // Shown from the file itself and uploaded only when the report is sent:
    // somebody who changes their mind should not have left a picture behind.
    setPictureFile(file)

    const reader = new FileReader()
    reader.onload = () => setPicture(String(reader.result))
    reader.readAsDataURL(file)
  }

  // A screenshot is on the clipboard, and asking for a saved file first is how
  // a report stops being written.
  const pasted = (event: React.ClipboardEvent) => {
    const image = [...event.clipboardData.items].find((item) =>
      item.type.startsWith("image/")
    )
    if (image) {
      event.preventDefault()
      take(image.getAsFile())
    }
  }

  const close = () => {
    setOpen(false)
    setTitle("")
    setDetail("")
    setPicture(null)
    setPictureFile(null)
    setKind("broken")
  }

  const submit = async () => {
    setSending(true)

    try {
      let media: string | null = null

      if (pictureFile) {
        const response = await fetch(
          `/api/media?name=${encodeURIComponent(pictureFile.name)}`,
          { method: "POST", body: pictureFile },
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

        media = ((await response.json()) as { id: string }).id
      }

      await api("POST /api/reports", {
        body: {
          kind,
          screen: window.location.pathname,
          // One paragraph: what it is in a line, and then anything else.
          body: [title, detail].filter(Boolean).join("\n\n"),
          environment: whatWentWrong(),
          media_id: media,
        },
      })

      toast.success(t`Said. Whoever runs this machine can see it.`)
      close()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setSending(false)
    }
  }

  return (
    <>
      <span onClick={() => setOpen(true)}>
        {trigger ?? (
          <Button variant="outline" size="sm">
            <MessageSquareWarning /> {t`Report a problem`}
          </Button>
        )}
      </span>

      <Dialog open={open} onOpenChange={(kept) => !kept && close()}>
        <DialogContent className="sm:max-w-lg" onPaste={pasted}>
          <DialogHeader>
            <DialogTitle>{t`Say what happened`}</DialogTitle>
          </DialogHeader>

          <div className="flex max-h-[60vh] flex-col gap-4 overflow-y-auto">
            {already && already.length > 0 && (
              <div className="flex flex-col gap-2 rounded-lg border border-border p-3">
                <p className="text-xs font-medium">{t`What you have said before`}</p>
                {already.slice(0, 5).map((one) => (
                  <div key={one.id} className="text-xs">
                    <Badge variant={one.answer ? "default" : "secondary"}>
                      {one.state}
                    </Badge>{" "}
                    <span>{one.body.split("\n")[0]}</span>
                    {one.answer ? (
                      <p className="text-muted-foreground">{one.answer}</p>
                    ) : null}
                  </div>
                ))}
              </div>
            )}

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="report-kind">{t`What kind of thing`}</Label>
              <Select
                value={kind}
                onValueChange={(value) =>
                  setKind((value as ReportKind) ?? "bug")
                }
              >
                <SelectTrigger id="report-kind" className="w-full">
                  <SelectValue>
                    {(value: string) => label[(value as ReportKind) ?? "broken"]}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  {(["broken", "missing", "wanted"] as const).map((one) => (
                    <SelectItem key={one} value={one}>
                      {label[one]}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="report-title">{t`In a line`}</Label>
              <Input
                id="report-title"
                autoFocus
                value={title}
                onChange={(event) => setTitle(event.target.value)}
                placeholder={t`The forms screen is empty`}
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="report-detail">{t`Anything else`}</Label>
              <Textarea
                id="report-detail"
                rows={4}
                value={detail}
                onChange={(event) => setDetail(event.target.value)}
                placeholder={t`What you were doing, and what you expected instead. Not required.`}
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <Label>{t`A picture of it`}</Label>
              {picture ? (
                <div className="relative w-fit">
                  <img
                    src={picture}
                    alt={t`The screenshot you attached`}
                    className="max-h-48 rounded-lg border border-border"
                  />
                  <Button
                    variant="secondary"
                    size="icon-sm"
                    aria-label={t`Remove the picture`}
                    className="absolute top-1 right-1"
                    onClick={() => setPicture(null)}
                  >
                    <X className="size-3.5" />
                  </Button>
                </div>
              ) : (
                <label className="inline-flex w-fit cursor-pointer items-center gap-1.5 rounded-md border border-input px-3 py-2 text-sm font-medium shadow-xs hover:bg-muted">
                  <ImagePlus className="size-4" />
                  {t`Choose one, or paste it`}
                  <input
                    type="file"
                    accept="image/*"
                    className="hidden"
                    onChange={(event) => {
                      take(event.target.files?.[0])
                      event.target.value = ""
                    }}
                  />
                </label>
              )}
            </div>

            <p className="text-xs text-muted-foreground">
              {t`Which browser you are using, which screen you were on and anything the panel has already recorded going wrong are sent with this. It goes to whoever runs this server, and nowhere else.`}
            </p>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={close}>
              {t`Cancel`}
            </Button>
            <Button
              disabled={sending || title.trim().length === 0}
              onClick={() => void submit()}
            >
              {sending ? <Loader2 className="animate-spin" /> : null}
              {t`Send`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
