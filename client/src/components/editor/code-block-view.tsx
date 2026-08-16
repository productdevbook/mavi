import * as React from "react"
import { NodeViewContent, NodeViewWrapper, type NodeViewProps } from "@tiptap/react"
import { useLingui } from "@lingui/react/macro"
import { Check, Copy } from "lucide-react"

import { CODE_LANGUAGES } from "@/components/editor/extensions/languages"

export function CodeBlockView({
  node,
  updateAttributes,
  extension,
}: NodeViewProps) {
  const { t } = useLingui()
  const [copied, setCopied] = React.useState(false)
  const language = (node.attrs.language as string) || "plaintext"

  const languages: string[] = React.useMemo(() => {
    const listed = extension.options.lowlight?.listLanguages?.() as
      | string[]
      | undefined
    return listed?.length ? listed : CODE_LANGUAGES
  }, [extension.options.lowlight])

  const copy = async () => {
    await navigator.clipboard.writeText(node.textContent)
    setCopied(true)
    window.setTimeout(() => setCopied(false), 1500)
  }

  return (
    <NodeViewWrapper className="mavi-code-block group relative">
      <div className="absolute top-2 right-2 z-10 flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
        <select
          contentEditable={false}
          value={language}
          onChange={(event) =>
            updateAttributes({ language: event.target.value })
          }
          className="h-6 rounded-md border border-border bg-background px-1.5 text-xs text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        >
          <option value="plaintext">{t`plain text`}</option>
          {languages.sort().map((item) => (
            <option key={item} value={item}>
              {item}
            </option>
          ))}
        </select>
        <button
          type="button"
          contentEditable={false}
          onClick={copy}
          aria-label={t`Copy code`}
          className="flex size-6 items-center justify-center rounded-md border border-border bg-background text-muted-foreground transition-colors hover:text-foreground"
        >
          {copied ? <Check className="size-3" /> : <Copy className="size-3" />}
        </button>
      </div>
      <pre spellCheck={false}>
        <NodeViewContent
          as={"code" as "div"}
          className={`language-${language}`}
        />
      </pre>
    </NodeViewWrapper>
  )
}
