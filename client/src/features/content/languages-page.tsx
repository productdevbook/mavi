import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Languages as LanguagesIcon, Plus, Star, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { nextApi, nextEvery } from "@/lib/server-next"
import { serverNextMessage } from "@/lib/server-next-auth"
import type { Language } from "@api-next"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
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
  DashboardEmpty,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

/**
 * Which languages a site writes in.
 *
 * Not the language of this panel, which is whoever is reading it: this is what
 * a post may be written in, and the one it is written in when nobody says.
 */
export function LanguagesPage() {
  const { t } = useLingui()
  const [languages, setLanguages] = React.useState<Language[] | null>(null)
  const [code, setCode] = React.useState("")
  const [name, setName] = React.useState("")
  const [going, setGoing] = React.useState<Language | null>(null)

  const load = React.useCallback(() => {
    nextEvery("languages.list", { query: {} })
      .then(setLanguages)
      .catch((why: unknown) => {
        toast.error(serverNextMessage(why))
        setLanguages([])
      })
  }, [])

  React.useEffect(() => load(), [load])

  const add = async () => {
    if (!code.trim()) return

    try {
      await nextApi("languages.create", {
        body: { tag: code.trim(), name: name.trim() || code.trim() },
      })
      setCode("")
      setName("")
      load()
    } catch (why) {
      toast.error(serverNextMessage(why))
    }
  }

  const makeDefault = async (language: Language) => {
    try {
      await nextApi("languages.update", {
        path: { tag: language.tag },
        body: { is_default: true },
      })
      load()
    } catch (why) {
      toast.error(serverNextMessage(why))
    }
  }

  const remove = async () => {
    if (!going) return

    try {
      await nextApi("languages.delete", { path: { tag: going.tag } })
      load()
    } catch (why) {
      toast.error(serverNextMessage(why))
    } finally {
      setGoing(null)
    }
  }

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`Languages`}
        description={t`The languages your content can be written in. This is separate from the language of this admin panel.`}
      />

      <form
        onSubmit={(event) => {
          event.preventDefault()
          void add()
        }}
        className="flex max-w-2xl flex-wrap gap-2"
      >
        <Input
          value={code}
          onChange={(event) => setCode(event.target.value)}
          placeholder={t`Code (en, de, pt-BR)`}
          className="w-44"
        />
        <Input
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder={t`Name`}
          className="w-52"
        />
        <Button type="submit">
          <Plus /> {t`Add language`}
        </Button>
      </form>

      {languages === null ? (
        <DashboardLoading />
      ) : languages.length === 0 ? (
        <DashboardEmpty
          icon={LanguagesIcon}
          title={t`No languages yet`}
          description={t`Add the first language your site's content can be written in.`}
        />
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {languages.map((language) => (
            <div
              key={language.tag}
              className="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-3"
            >
              <div className="min-w-0 basis-full sm:flex-1 sm:basis-0">
                <div className="flex flex-wrap items-center gap-2">
                  <p className="truncate text-sm font-medium">
                    {language.name}
                  </p>
                  <Badge variant="secondary">{language.tag}</Badge>
                  {language.is_default && (
                    <Badge>
                      <Star className="size-3" /> {t`Default`}
                    </Badge>
                  )}
                </div>
              </div>

              <Button
                variant="ghost"
                size="sm"
                className="ml-auto shrink-0"
                disabled={language.is_default}
                onClick={() => void makeDefault(language)}
              >
                {t`Make default`}
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Delete`}
                disabled={language.is_default}
                onClick={() => setGoing(language)}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      <AlertDialog
        open={going !== null}
        onOpenChange={(open) => !open && setGoing(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Remove this language?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`Content already written in this language blocks removal — you'll be told how much there is.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void remove()}>
              {t`Remove`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
