import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Check, Copy, KeyRound, Loader2, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Key } from "@api"
import { AssistantClients } from "@/components/assistant-clients"
import { McpConnection } from "@/components/mcp-connection"
import {
  DashboardEmpty,
  DashboardError,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

function Copyable({ text, label }: { text: string; label: string }) {
  const { t } = useLingui()
  const [copied, setCopied] = React.useState(false)

  const copy = () => {
    void navigator.clipboard.writeText(text).then(
      () => {
        setCopied(true)
        setTimeout(() => setCopied(false), 1500)
      },
      () => toast.error(t`Could not copy it`)
    )
  }

  return (
    <div className="relative">
      <pre className="overflow-x-auto rounded-xl border border-border bg-muted/40 px-4 py-3 pr-12 text-xs">
        {text}
      </pre>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={label}
        className="absolute top-2 right-2"
        onClick={copy}
      >
        {copied ? <Check /> : <Copy />}
      </Button>
    </div>
  )
}

export function ApiPage() {
  const { t } = useLingui()
  const [keys, setKeys] = React.useState<Key[] | null>(null)
  const [error, setError] = React.useState(false)
  const [name, setName] = React.useState("")
  const [creating, setCreating] = React.useState(false)
  const [issued, setIssued] = React.useState<string | null>(null)
  const [revoking, setRevoking] = React.useState<string | null>(null)

  const origin = window.location.origin

  const load = React.useCallback(() => {
    setError(false)
    api("keys.list")
      .then(setKeys)
      .catch((why: unknown) => {
        toast.error(said(why))
        setError(true)
        setKeys([])
      })
  }, [])

  React.useEffect(load, [load])

  const createKey = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setCreating(true)

    try {
      const made = await api("keys.make", { body: { name: name.trim() } })
      setIssued(made.token)
      setName("")
      load()
      toast.success(t`The key was made.`)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setCreating(false)
    }
  }

  const revoke = async (key: Key) => {
    setRevoking(key.id)

    try {
      await api("keys.end", { path: { id: key.id } })
      load()
      toast.success(t`The key was taken back.`)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setRevoking(null)
    }
  }

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <DashboardPageHeader
        title={t`API`}
        description={t`Everything this panel does, something else can do. The address is the one you reached this site on.`}
      />

      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-medium">{t`What is published`}</h2>
        <p className="text-sm text-muted-foreground">
          {t`Read without an account: what is published, and nothing else.`}
        </p>
        <Copyable
          label={t`Copy the public site command`}
          text={`curl ${origin}/api/open/site`}
        />
        <Copyable
          label={t`Copy the products command`}
          text={`curl ${origin}/api/open/products`}
        />
      </section>

      <section className="flex flex-col gap-4 rounded-xl border border-border p-4">
        <McpConnection origin={origin} name="mavi" token={issued}>
          <AssistantClients url={`${origin}/mcp`} />
        </McpConnection>
      </section>

      <section className="flex flex-col gap-4 rounded-xl border border-border p-4">
        <div className="flex items-start gap-3">
          <KeyRound className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          <div>
            <h2 className="text-sm font-medium">{t`Assistant keys`}</h2>
            <p className="text-sm text-muted-foreground">
              {t`A key carries the grants of the person who made it. Name each one so it can be identified and revoked later.`}
            </p>
          </div>
        </div>

        <form
          className="flex flex-col gap-2 sm:flex-row sm:items-end"
          onSubmit={(event) => void createKey(event)}
        >
          <div className="flex flex-1 flex-col gap-1.5">
            <Label htmlFor="assistant-key-name">{t`Name`}</Label>
            <Input
              id="assistant-key-name"
              value={name}
              placeholder={t`For example, website assistant`}
              onChange={(event) => setName(event.target.value)}
            />
          </div>
          <Button type="submit" disabled={creating || !name.trim()}>
            {creating ? <Loader2 className="animate-spin" /> : <Plus />}
            {t`Make a key`}
          </Button>
        </form>

        {issued ? (
          <div className="flex flex-col gap-2 rounded-xl border border-amber-500/40 bg-amber-500/5 p-3">
            <p className="text-sm font-medium">{t`Copy this key now. It is shown once.`}</p>
            <p className="text-xs text-muted-foreground">
              {t`The server stores only a hash. If this value is lost, revoke the key and make another.`}
            </p>
            <Copyable label={t`Copy the new key`} text={issued} />
          </div>
        ) : null}

        {keys === null ? (
          <DashboardLoading />
        ) : error ? (
          <DashboardError message={t`The keys could not be read just now.`} />
        ) : keys.length === 0 ? (
          <DashboardEmpty
            icon={KeyRound}
            title={t`No assistant keys yet.`}
            description={t`Make one when a terminal or deployment needs a non-browser connection.`}
          />
        ) : (
          <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
            {keys.map((key) => (
              <div
                key={key.id}
                className="flex flex-wrap items-center gap-3 px-4 py-3"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">{key.name}</p>
                  <p className="text-xs text-muted-foreground">
                    {key.last_seen_at
                      ? t`Last used ${new Date(key.last_seen_at).toLocaleString()}`
                      : t`Not used yet`}
                  </p>
                </div>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t`Take this key back`}
                  title={t`Take this key back`}
                  disabled={revoking === key.id}
                  onClick={() => void revoke(key)}
                >
                  {revoking === key.id ? (
                    <Loader2 className="animate-spin" />
                  ) : (
                    <Trash2 />
                  )}
                </Button>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  )
}
