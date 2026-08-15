import { marked } from "marked"
import TurndownService from "turndown"

const turndown = new TurndownService({
  headingStyle: "atx",
  hr: "---",
  bulletListMarker: "-",
  codeBlockStyle: "fenced",
  fence: "```",
  emDelimiter: "*",
  strongDelimiter: "**",
  linkStyle: "inlined",
})

turndown.addRule("strikethrough", {
  filter: ["del", "s"],
  replacement: (content) => `~~${content}~~`,
})

turndown.addRule("underline", {
  filter: ["u"],
  replacement: (content) => `<u>${content}</u>`,
})

turndown.addRule("highlight", {
  filter: ["mark"],
  replacement: (content) => `==${content}==`,
})

turndown.addRule("taskListItem", {
  filter: (node) =>
    node.nodeName === "LI" && node.getAttribute("data-checked") !== null,
  replacement: (content, node) => {
    const checked = (node as HTMLElement).getAttribute("data-checked") === "true"
    const body = content
      .replace(/^\n+/, "")
      .replace(/\n+$/, "")
      .replace(/\n/gm, "\n  ")
    return `- [${checked ? "x" : " "}] ${body}\n`
  },
})

turndown.addRule("codeBlockWithLanguage", {
  filter: (node) =>
    node.nodeName === "PRE" && node.firstChild?.nodeName === "CODE",
  replacement: (_content, node) => {
    const code = node.firstChild as HTMLElement
    const language =
      code.getAttribute("class")?.match(/language-(\S+)/)?.[1] ?? ""
    return `\n\n\`\`\`${language}\n${code.textContent ?? ""}\n\`\`\`\n\n`
  },
})

turndown.addRule("table", {
  filter: "table",
  replacement: (_content, node) => {
    const rows = Array.from((node as HTMLTableElement).querySelectorAll("tr"))
    if (!rows.length) return ""

    const cellsOf = (row: HTMLTableRowElement) =>
      Array.from(row.querySelectorAll("th,td")).map((cell) =>
        (cell.textContent ?? "").replace(/\|/g, "\\|").replace(/\n/g, " ").trim()
      )

    const header = cellsOf(rows[0] as HTMLTableRowElement)
    const body = rows.slice(1).map((row) => cellsOf(row as HTMLTableRowElement))
    const line = (cells: string[]) => `| ${cells.join(" | ")} |`

    return [
      "",
      line(header),
      line(header.map(() => "---")),
      ...body.map(line),
      "",
    ].join("\n")
  },
})

turndown.addRule("iframeEmbed", {
  filter: (node) =>
    node.nodeName === "DIV" && node.getAttribute("data-youtube-video") !== null,
  replacement: (_content, node) => {
    const src = (node as HTMLElement).querySelector("iframe")?.getAttribute("src")
    return src ? `\n\n[${src}](${src})\n\n` : ""
  },
})

export function htmlToMarkdown(html: string): string {
  return turndown.turndown(html).replace(/\n{3,}/g, "\n\n").trim()
}

export function markdownToHtml(markdown: string): string {
  return marked.parse(markdown, { async: false, gfm: true, breaks: false })
}
