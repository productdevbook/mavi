import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Bot, ExternalLink, FileText, Map } from "lucide-react"

import { Panel } from "@/components/charts"

/**
 * The three files that make a site findable, with a live check on each.
 *
 * They are served on the site's own address and the panel runs there too,
 * so a same-origin request answers whether each one is really up — the
 * owner sees green, or sees what to ask about. The sitemap goes by two
 * customary names; whichever answers is the one linked.
 */
export function Discoverability() {
  const { t } = useLingui()
  const [state, setState] = React.useState<Record<string, string | null>>({})

  React.useEffect(() => {
    let current = true
    const probe = async (paths: string[]) => {
      for (const path of paths) {
        try {
          const found = await fetch(path, { method: "HEAD" })
          if (found.ok) return path
        } catch {
          // unreachable counts the same as missing
        }
      }
      return null
    }
    Promise.all([
      probe(["/robots.txt"]),
      probe(["/sitemap.xml", "/sitemap-index.xml", "/sitemap_index.xml"]),
      probe(["/llms.txt"]),
    ]).then(([robots, sitemap, llms]) => {
      if (current) setState({ robots, sitemap, llms })
    })
    return () => {
      current = false
    }
  }, [])

  const rows = [
    {
      key: "robots",
      icon: FileText,
      name: "robots.txt",
      what: t`Tells search engines they are welcome, and where the map is.`,
    },
    {
      key: "sitemap",
      icon: Map,
      name: t`Sitemap`,
      what: t`Every page, listed for Google and the other crawlers.`,
    },
    {
      key: "llms",
      icon: Bot,
      name: "llms.txt",
      what: t`The site introduced to AI assistants, page by page.`,
    },
  ]

  return (
    <Panel title={t`Search & AI`} aside={t`on your own address`}>
      <ul className="divide-y divide-border">
        {rows.map((row) => {
          const at = state[row.key]
          const checked = row.key in state
          return (
            <li key={row.key} className="flex items-center gap-3 py-2.5">
              <row.icon className="size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <p className="text-sm font-medium">{row.name}</p>
                <p className="truncate text-xs text-muted-foreground">{row.what}</p>
              </div>
              {!checked ? (
                <span className="text-xs text-muted-foreground">…</span>
              ) : at ? (
                <a
                  href={at}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 rounded-md bg-emerald-500/10 px-2 py-1 text-xs font-medium text-emerald-600 hover:bg-emerald-500/20 dark:text-emerald-400"
                >
                  {t`Live`}
                  <ExternalLink className="size-3" />
                </a>
              ) : (
                <span className="rounded-md bg-amber-500/10 px-2 py-1 text-xs font-medium text-amber-600 dark:text-amber-400">
                  {t`Appears on next publish`}
                </span>
              )}
            </li>
          )
        })}
      </ul>
    </Panel>
  )
}
