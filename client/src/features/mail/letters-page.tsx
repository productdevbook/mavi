import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Mails, RotateCcw } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { useLanguages } from "@/lib/use-languages"
import type { Letter } from "@api"
import {
  DashboardEmpty,
  DashboardError,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"
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

/**
 * The mail this site sends one person.
 *
 * An invitation, a password link, a receipt. Written once and sent by the
 * machine rather than by somebody pressing send — which is why what can be put
 * in one is a fixed list: a letter naming something that is not there would be
 * a letter that fails on the day it matters.
 */
export function LettersPage() {
  const { t } = useLingui()
  const { languages, defaultCode } = useLanguages()

  const [chosen, setChosen] = React.useState("")
  const language = chosen || defaultCode

  const [letters, setLetters] = React.useState<Letter[] | null>(null)
  const [error, setError] = React.useState(false)
  const [drafts, setDrafts] = React.useState<
    Record<string, { subject: string; body: string }>
  >({})
  const [busy, setBusy] = React.useState<string | null>(null)

  const load = React.useCallback(() => {
    if (!language) return

    setError(false)
    api("letters.list", { query: { language } })
      .then((found) => {
        setLetters(found)
        setDrafts({})
      })
      .catch((why: unknown) => {
        toast.error(said(why))
        setError(true)
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
      await api("letters.write", {
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
      await api("letters.forget", {
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
    <div className="flex max-w-3xl flex-col gap-5">
      <DashboardPageHeader
        title={t`Letters`}
        description={t`What this site writes to one person: an invitation, a password link, a receipt. Anything in braces is filled in when it is sent.`}
        actions={
          languages.length > 1 ? (
            <Select
              value={language}
              onValueChange={(value) => setChosen(value ?? "")}
            >
              <SelectTrigger className="w-44">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {languages.map((one) => (
                  <SelectItem key={one.tag} value={one.tag}>
                    {one.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : null
        }
      />

      {letters === null ? (
        <DashboardLoading />
      ) : error ? (
        <DashboardError message={t`The letters could not be read just now.`} />
      ) : letters.length === 0 ? (
        <DashboardEmpty
          icon={Mails}
          title={t`No letters yet.`}
          description={t`This site has no automatic messages to customize.`}
        />
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
