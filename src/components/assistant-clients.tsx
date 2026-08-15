import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Check, Copy, ExternalLink, TriangleAlert } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"

/**
 * Where, in each program, the address goes.
 *
 * "Paste it wherever it asks for a server" is true and useless if you have
 * never seen the dialog. The wording is kept structural — the menu a thing
 * lives under, not the pixel it is at — and each one links to its own
 * documentation, because a menu that moves leaves the link working and this
 * page wrong.
 */

/** Programs that connect from their own servers rather than from this machine. */
const FROM_THE_CLOUD = ["claude", "chatgpt"]

interface Client {
  id: string
  name: string
  /** Where the address is pasted, in that program's own words. */
  steps: string[]
  /** For the ones where it is a command rather than a dialog. */
  command?: (url: string) => string
  documentation: string
}

/**
 * The list, built inside a hook so the strings are ones lingui can see.
 *
 * Passing `t` into a plain function looks equivalent and is not: the macro
 * rewrites `t` template literals where they are written, so strings in a
 * helper it cannot see are never extracted and never translated.
 */
function useClients(): Client[] {
  const { t } = useLingui()

  return [
    {
      id: "claude",
      name: "Claude",
      steps: [
        t`Settings, then Connectors.`,
        t`Add custom connector, and paste the address.`,
        t`The same connector then works in Claude on the web, on the desktop and on a phone.`,
      ],
      documentation:
        "https://support.claude.com/en/articles/11175166-get-started-with-custom-connectors-using-remote-mcp",
    },
    {
      id: "chatgpt",
      name: "ChatGPT",
      steps: [
        t`Settings, then Connectors, then Advanced settings at the bottom: turn on developer mode.`,
        t`Back in Connectors, create a custom connector and paste the address.`,
        t`A paid plan is needed for this; developer mode is where custom connectors live.`,
      ],
      documentation:
        "https://help.openai.com/en/articles/12584461-developer-mode-and-mcp-apps-in-chatgpt",
    },
    {
      id: "codex",
      name: "Codex",
      steps: [
        t`Run this. It notices that the address asks for a sign-in and opens one.`,
        t`Afterwards, /mcp lists what it is connected to.`,
      ],
      command: (url: string) => `codex mcp add mavi --url ${url}`,
      documentation: "https://developers.openai.com/codex/mcp",
    },
    {
      id: "claude-code",
      name: "Claude Code",
      steps: [t`Run this, then /mcp to sign in.`],
      command: (url: string) =>
        `claude mcp add --transport http mavi ${url}`,
      documentation:
        "https://modelcontextprotocol.io/docs/develop/connect-remote-servers",
    },
    {
      id: "cursor",
      name: "Cursor",
      steps: [
        t`Settings, then Tools & MCP, then New MCP Server — which opens the file below.`,
      ],
      command: (url: string) =>
        JSON.stringify({ mcpServers: { mavi: { url } } }, null, 2),
      documentation: "https://docs.cursor.com/context/mcp",
    },
  ]
}

function Copyable({ text }: { text: string }) {
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
        aria-label={t`Copy`}
        onClick={copy}
      >
        {copied ? <Check /> : <Copy />}
      </Button>
    </div>
  )
}

/**
 * Whether this address is one somebody else's servers could reach.
 *
 * Claude and ChatGPT connect from their own infrastructure, so a site being
 * looked at on a laptop is not a site they can see. It is the failure that
 * looks most like a broken feature and is hardest to guess at, so it is said
 * before it happens rather than left to be discovered.
 */
function onlyOnThisMachine(origin: string): boolean {
  try {
    const { hostname, protocol } = new URL(origin)
    if (protocol !== "https:") return true
    return (
      hostname === "localhost" ||
      hostname.endsWith(".localhost") ||
      !hostname.includes(".")
    )
  } catch {
    return false
  }
}

export function AssistantClients({ url }: { url: string }) {
  const { t } = useLingui()
  const all = useClients()
  const [chosen, setChosen] = React.useState(all[0].id)

  const client = all.find((one) => one.id === chosen) ?? all[0]
  const unreachable = onlyOnThisMachine(url) && FROM_THE_CLOUD.includes(client.id)

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap gap-1.5">
        {all.map((one) => (
          <Button
            key={one.id}
            variant={one.id === chosen ? "secondary" : "ghost"}
            size="sm"
            onClick={() => setChosen(one.id)}
          >
            {one.name}
          </Button>
        ))}
      </div>

      <ol className="flex flex-col gap-1.5 text-sm text-muted-foreground">
        {client.steps.map((step) => (
          <li key={step} className="flex gap-2">
            <span aria-hidden>·</span>
            <span>{step}</span>
          </li>
        ))}
      </ol>

      {client.command && <Copyable text={client.command(url)} />}

      {unreachable && (
        <div className="flex gap-2 rounded-lg border border-border bg-muted/40 px-3 py-2.5">
          <TriangleAlert className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          <p className="text-sm text-muted-foreground">
            {t`${client.name} connects from its own servers, not from this computer, so it cannot reach this address. It will work once the site is on a public address with https. The programs that run on your machine — Codex, Claude Code, Cursor — can connect to it as it is.`}
          </p>
        </div>
      )}

      <a
        href={client.documentation}
        target="_blank"
        rel="noreferrer"
        className="inline-flex items-center gap-1.5 text-sm text-muted-foreground underline"
      >
        <ExternalLink className="size-3.5" />
        {t`${client.name}'s own instructions`}
      </a>
    </div>
  )
}
