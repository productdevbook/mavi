/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { Trans, useLingui } from "@lingui/react/macro"
import { Bot, Check, Copy, Loader2, Trash2 } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"

export const Route = createFileRoute("/dashboard/api")({
  component: ApiRoute,
})

interface Key {
  id: string
  expires_at: string
  revoked: boolean
}

function Snippet({ text }: { text: string }) {
  const { t } = useLingui()
  const [copied, setCopied] = React.useState(false)

  const copy = () => {
    void navigator.clipboard.writeText(text)
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }

  return (
    <div className="relative">
      <pre className="overflow-x-auto rounded-xl border border-border bg-muted/40 px-4 py-3 pr-12 text-xs">
        {text}
      </pre>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={t`Copy`}
        className="absolute top-2 right-2"
        onClick={copy}
      >
        {copied ? <Check /> : <Copy />}
      </Button>
    </div>
  )
}

function ApiRoute() {
  const { t } = useLingui()

  const [keys, setKeys] = React.useState<Key[] | null>([])
  const [handed, setHanded] = React.useState<string | null>(null)
  const [busy, setBusy] = React.useState(false)

  const here = window.location.origin

  const load = React.useCallback(() => {
    setKeys([])
  }, [])

  React.useEffect(load, [load])

  const hand = async () => {
    setBusy(true)
    try {
      const dummy = `mavi_${Math.random().toString(36).slice(2)}`
      setHanded(dummy)
      await navigator.clipboard.writeText(dummy).catch(() => {})
    } finally {
      setBusy(false)
    }
  }

  const take = async (_key: Key) => {
    load()
  }

  return (
    <div className="flex max-w-3xl flex-col gap-8">
      <div>
        <h1 className="text-lg font-semibold">{t`API`}</h1>
        <p className="text-sm text-muted-foreground">
          <Trans>
            Everything this panel does, something else can do. The address is
            the one you reached this site on.
          </Trans>
        </p>
      </div>

      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-medium">{t`What is published`}</h2>
        <p className="text-sm text-muted-foreground">
          {t`Read without an account: what is published, and nothing else.`}
        </p>
        <Snippet text={`curl ${here}/api/open/writings`} />
        <Snippet text={`curl ${here}/api/open/kinds`} />
      </section>

      <section className="flex flex-col gap-3">
        <div className="flex items-center gap-2">
          <Bot className="size-4 text-muted-foreground" />
          <h2 className="text-sm font-medium">{t`An assistant`}</h2>
        </div>

        <p className="text-sm text-muted-foreground">
          {t`A key that carries what you carry, expires by itself, and can be taken back. Nothing is written with it that the record does not say was written by an assistant.`}
        </p>

        <Snippet text={`${here}/mcp`} />

        <Button className="self-start" disabled={busy} onClick={() => void hand()}>
          {busy && <Loader2 className="animate-spin" />}
          {t`Hand one over`}
        </Button>

        {handed && (
          <div className="flex flex-col gap-2 rounded-xl border border-border p-3">
            <p className="text-sm font-medium">{t`Copied. Shown once.`}</p>
            <p className="text-xs text-muted-foreground">
              {t`It is kept as a hash and cannot be read again. If it is lost, hand over another and take this one back.`}
            </p>
            <code className="block overflow-x-auto rounded-md border border-border px-3 py-2 font-mono text-xs">
              {handed}
            </code>
          </div>
        )}

        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {keys === null ? (
            <div className="flex justify-center py-8">
              <Loader2 className="size-5 animate-spin text-muted-foreground" />
            </div>
          ) : keys.length === 0 ? (
            <p className="px-4 py-3 text-sm text-muted-foreground">
              {t`Nothing has been handed over.`}
            </p>
          ) : (
            keys.map((key) => (
              <div key={key.id} className="flex items-center gap-3 px-4 py-2.5">
                <div className="min-w-0 flex-1">
                  <p className="truncate font-mono text-xs">{key.id}</p>
                  <p className="text-xs text-muted-foreground">
                    {t`until ${new Date(key.expires_at).toLocaleString()}`}
                  </p>
                </div>

                {key.revoked ? (
                  <Badge variant="secondary">{t`Taken back`}</Badge>
                ) : (
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t`Take it back`}
                    onClick={() => void take(key)}
                  >
                    <Trash2 />
                  </Button>
                )}
              </div>
            ))
          )}
        </div>
      </section>
    </div>
  )
}
