/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Mails, RotateCcw } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { useLanguages } from "@/lib/use-languages"
import type { Letter } from "../../server/types/mavicms"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
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

export const Route = createFileRoute("/dashboard/letters")({
  component: LettersRoute,
})

/**
 * The mail this site sends one person.
 *
 * An invitation, a password link, a receipt. Written once and sent by the
 * machine rather than by somebody pressing send — which is why what can be put
 * in one is a fixed list: a letter naming something that is not there would be
 * a letter that fails on the day it matters.
 */
function LettersRoute() {
  const { t } = useLingui()
  const { languages, defaultCode } = useLanguages()

  const [chosen, setChosen] = React.useState("")
  const language = chosen || defaultCode

  const [letters, setLetters] = React.useState<Letter[] | null>(null)
  const [drafts, setDrafts] = React.useState<
    Record<string, { subject: string; body: string }>
  >({})
  const [busy, setBusy] = React.useState<string | null>(null)

  const load = React.useCallback(() => {
    if (!language) return

    api("GET /api/mail/letters", { query: { language } })
      .then((found) => {
        setLetters(found)
        setDrafts({})
      })
      .catch((why: unknown) => {
        toast.error(said(why))
        setLetters((held) => held ?? [])
      })
  }, [language])

  React.useEffect(load, [load])

  const save = async (letter: Letter) => {
    setBusy(letter.kind)

    const draft = drafts[letter.kind] ?? {
      subject: letter.subject,
      body: letter.body,
    }

    try {
      await api("PUT /api/mail/letters/{kind}", {
        path: { kind: letter.kind },
        body: { language, subject: draft.subject, body: draft.body },
      })
      load()
      toast.success(t`Saved`)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(null)
    }
  }

  const back = async (letter: Letter) => {
    setBusy(letter.kind)

    try {
      await api("DELETE /api/mail/letters/{kind}", {
        path: { kind: letter.kind },
        query: { language },
      })
      load()
      toast.success(t`Back to the words this build ships with.`)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(null)
    }
  }

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">{t`Letters`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`What this site writes to one person: an invitation, a password link, a receipt. Anything in braces is filled in when it is sent.`}
          </p>
        </div>

        {languages.length > 1 && (
          <Select
            value={language}
            onValueChange={(value) => setChosen(value ?? "")}
          >
            <SelectTrigger className="w-44">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {languages.map((one) => (
                <SelectItem key={one.code} value={one.code}>
                  {one.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </div>

      {letters === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : (
        letters.map((letter) => {
          const draft = drafts[letter.kind] ?? {
            subject: letter.subject,
            body: letter.body,
          }

          return (
            <section
              key={letter.kind}
              className="flex flex-col gap-3 rounded-xl border border-border p-4"
            >
              <div className="flex flex-wrap items-center gap-2">
                <Mails className="size-4 text-muted-foreground" />
                <h2 className="text-sm font-medium">{letter.kind}</h2>
                <Badge variant={letter.theirs ? "default" : "secondary"}>
                  {letter.theirs ? t`Yours` : t`As it comes`}
                </Badge>

                {letter.theirs && (
                  <Button
                    variant="ghost"
                    size="sm"
                    className="ml-auto"
                    disabled={busy === letter.kind}
                    onClick={() => void back(letter)}
                  >
                    <RotateCcw /> {t`Put it back`}
                  </Button>
                )}
              </div>

              <div className="flex flex-col gap-1.5">
                <Label htmlFor={`subject-${letter.kind}`}>{t`Subject`}</Label>
                <Input
                  id={`subject-${letter.kind}`}
                  value={draft.subject}
                  onChange={(event) =>
                    setDrafts((held) => ({
                      ...held,
                      [letter.kind]: { ...draft, subject: event.target.value },
                    }))
                  }
                />
              </div>

              <div className="flex flex-col gap-1.5">
                <Label htmlFor={`body-${letter.kind}`}>{t`What it says`}</Label>
                <Textarea
                  id={`body-${letter.kind}`}
                  rows={6}
                  value={draft.body}
                  onChange={(event) =>
                    setDrafts((held) => ({
                      ...held,
                      [letter.kind]: { ...draft, body: event.target.value },
                    }))
                  }
                />
                <p className="text-xs text-muted-foreground">
                  {t`It can name: ${letter.names.map((name) => `{${name}}`).join(", ")}`}
                </p>
              </div>

              <Button
                className="self-start"
                disabled={busy === letter.kind}
                onClick={() => void save(letter)}
              >
                {busy === letter.kind && <Loader2 className="animate-spin" />}
                {t`Save`}
              </Button>
            </section>
          )
        })
      )}
    </div>
  )
}
