import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Loader2, MessageSquareWarning } from "lucide-react"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
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
import { api, every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { FeedbackReport, ReportKind } from "@api"

/** Saying something is wrong, missing or wanted is a site-scoped mutation. */
export function ReportAProblem({ trigger }: { trigger?: React.ReactNode }) {
  const { t } = useLingui()
  const [open, setOpen] = React.useState(false)
  const [kind, setKind] = React.useState<ReportKind>("broken")
  const [title, setTitle] = React.useState("")
  const [detail, setDetail] = React.useState("")
  const [sending, setSending] = React.useState(false)
  const [already, setAlready] = React.useState<FeedbackReport[] | null>(null)

  const label: Record<ReportKind, string> = {
    broken: t`Something is broken`,
    missing: t`It cannot do something I need`,
    wanted: t`I would like something`,
  }

  React.useEffect(() => {
    if (!open) return
    let current = true
    setAlready(null)
    void every("feedback.reports.list", { query: { limit: 5 } })
      .then((reports) => {
        if (current) setAlready(reports)
      })
      .catch(() => {
        // Reading the inbox is an administrative permission. A user can
        // still create a report when the server does not grant that view.
        if (current) setAlready([])
      })
    return () => {
      current = false
    }
  }, [open])

  const close = () => {
    setOpen(false)
    setTitle("")
    setDetail("")
    setKind("broken")
  }

  const submit = async () => {
    setSending(true)
    try {
      await api("feedback.reports.create", {
        body: {
          kind,
          title: title.trim(),
          body: detail.trim(),
          context: {
            screen: window.location.pathname,
            browser: navigator.userAgent,
            language: navigator.language,
          },
        },
      })
      toast.success(t`Your report was sent to whoever runs this server.`)
      close()
    } catch (why) {
      toast.error(apiMessage(why))
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
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>{t`Say what happened`}</DialogTitle>
          </DialogHeader>

          <div className="flex max-h-[60vh] flex-col gap-4 overflow-y-auto">
            {already && already.length > 0 && (
              <div className="flex flex-col gap-2 rounded-lg border border-border p-3">
                <p className="text-xs font-medium">{t`What you have said before`}</p>
                {already.map((one) => (
                  <div key={one.id} className="text-xs">
                    <Badge variant={one.answer ? "default" : "secondary"}>
                      {one.state}
                    </Badge>{" "}
                    <span>{one.title}</span>
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
                onValueChange={(value) => setKind(value as ReportKind)}
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

            <p className="text-xs text-muted-foreground">
              {t`The screen, browser and language are sent with this to whoever runs this server, and nowhere else.`}
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
