import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Monitor, Smartphone } from "lucide-react"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"

/** What a placeholder is filled in with so the preview looks like a letter. */
const STAND_INS: Record<string, string> = {
  content:
    "<h2>Buraya kampanyanın kendisi gelir</h2><p>The campaign's own body goes here. This paragraph is a stand-in so the letterhead can be read as a letter rather than as a hole.</p>",
  name: "Ayşe Yılmaz",
  email: "ayse@example.com",
  unsubscribe_url: "#",
  site_title: "…",
  year: String(new Date().getFullYear()),
}

/**
 * Fills the placeholders in so somebody can see the letter rather than the
 * template. The real substitution happens on the server; this only has to
 * agree with it about the spelling.
 */
function withStandIns(body: string): string {
  return body.replace(/\{\{\s*([\w.]+)\s*\}\}/g, (whole, name: string) => {
    const held = STAND_INS[name]
    return held ?? whole
  })
}

/**
 * The letterhead, and what it looks like, side by side.
 *
 * Most people arriving here have HTML somebody or something else wrote and
 * want to know whether it survives being an email — which is a question a
 * textarea cannot answer. The preview is an iframe with nothing of this
 * application in it, because a template that inherits the panel's own styles
 * looks right here and wrong in everybody's inbox.
 */
export function LetterheadEditor({
  value,
  onChange,
}: {
  value: string
  onChange: (value: string) => void
}) {
  const { t } = useLingui()
  const [narrow, setNarrow] = React.useState(false)

  const shown = React.useMemo(() => withStandIns(value), [value])

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <p className="text-sm font-medium">{t`The letterhead`}</p>
        <div className="flex gap-1">
          <Button
            type="button"
            variant={narrow ? "ghost" : "outline"}
            size="icon-sm"
            aria-label={t`As on a computer`}
            onClick={() => setNarrow(false)}
          >
            <Monitor />
          </Button>
          <Button
            type="button"
            variant={narrow ? "outline" : "ghost"}
            size="icon-sm"
            aria-label={t`As on a phone`}
            onClick={() => setNarrow(true)}
          >
            <Smartphone />
          </Button>
        </div>
      </div>

      <div className="grid gap-3 lg:grid-cols-2">
        <Textarea
          rows={20}
          className="font-mono text-xs"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          placeholder={t`Paste the HTML here`}
        />

        <div className="overflow-hidden rounded-xl border border-border bg-white">
          <iframe
            title={t`What it will look like`}
            // Nothing of this page reaches inside, and nothing inside reaches
            // out: a letterhead is somebody else's HTML, often written by a
            // model, and it is not to be trusted with a script or a form.
            sandbox=""
            srcDoc={shown}
            className={cn(
              "h-[30rem] border-0 bg-white transition-all",
              narrow ? "mx-auto w-[22rem]" : "w-full"
            )}
          />
        </div>
      </div>

      <p className="text-sm text-muted-foreground">
        {t`{{ content }} is where the campaign goes. {{ name }}, {{ email }} and {{ unsubscribe_url }} are filled in for each person, along with any field you keep about them. The preview fills them with stand-ins.`}
      </p>
    </div>
  )
}
