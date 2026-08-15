import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Check, Copy } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"

/**
 * How to point an assistant at this address.
 *
 * The token is not shown here — it is made just above, once, and this is
 * written with a placeholder so that what somebody copies is safe to paste
 * into a message while they work out where it goes.
 */

const PLACEHOLDER = "…"

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
      <pre className="overflow-x-auto rounded-lg border border-border bg-muted/40 px-3 py-2.5 pr-10 text-xs">
        {text}
      </pre>
      <Button
        variant="ghost"
        size="icon-sm"
        className="absolute right-1 top-1"
        aria-label={label}
        onClick={copy}
      >
        {copied ? <Check /> : <Copy />}
      </Button>
    </div>
  )
}

export function McpConnection({
  origin,
  name,
  token,
  children,
}: {
  origin: string
  /** What the connection is called in the client's settings. */
  name: string
  /** A token to write into the snippets, when one has just been made. */
  token?: string | null
  children?: React.ReactNode
}) {
  const { t } = useLingui()
  const url = `${origin}/api/mcp`
  const secret = token ?? PLACEHOLDER

  return (
    <section className="flex flex-col gap-3">
      <div>
        <h2 className="text-sm font-medium">{t`Assistants`}</h2>
        <p className="text-sm text-muted-foreground">
          {t`This address answers the Model Context Protocol, so an assistant can be pointed at it and asked to do the work rather than told how.`}
        </p>
      </div>

      {children}

      <Copyable text={url} label={t`Copy the address`} />

      <p className="text-sm text-muted-foreground">
        {t`Paste it wherever a program asks for a server address and nothing else. You will be sent here to sign in and asked whether to allow it, and it will then be able to do what you can — writing included. This is the way to connect.`}
      </p>

      <details className="rounded-lg border border-border px-3 py-2">
        <summary className="cursor-pointer text-sm text-muted-foreground">
          {t`Connecting with a token instead, from a terminal`}
        </summary>

        <div className="flex flex-col gap-3 pt-3">
          <p className="text-sm text-muted-foreground">
            {t`A build token can read this site and change nothing, so an assistant connected with one is offered only the tools that read: it will not be able to write a post, upload a file or publish. That is usually not what you want here — sign in above instead.`}
          </p>

          <Copyable
            text={`claude mcp add --transport http ${name} ${url} \\
  --header "Authorization: Bearer ${secret}"`}
            label={t`Copy the command`}
          />

          <Copyable
            text={JSON.stringify(
              {
                mcpServers: {
                  [name]: {
                    type: "http",
                    url,
                    headers: { Authorization: `Bearer ${secret}` },
                  },
                },
              },
              null,
              2
            )}
            label={t`Copy the configuration`}
          />

          {!token && (
            <p className="text-sm text-muted-foreground">
              {t`Put a token where the … is. It is shown once, when you make it.`}
            </p>
          )}
        </div>
      </details>
    </section>
  )
}
