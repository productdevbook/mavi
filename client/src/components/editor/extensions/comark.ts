import type { JSONContent, MarkdownRendererHelpers } from "@tiptap/core"

/**
 * Markdown output that [comark](https://comark.dev) can turn into components.
 *
 * Two shapes, each used where it holds up:
 *
 * **Blocks** are directives — `:::youtube {src="…"}` — which comark reads as a
 * named node with props. Tiptap already writes these; nothing here changes them.
 *
 * **Inline marks** that Markdown has no syntax for are written as inline HTML.
 * Tiptap's own `==highlight==` and `++underline++` are not Markdown and comark
 * leaves them as literal text. An inline directive would be understood, but
 * only with whitespace in front of it: `H:sub[2]O` breaks where
 * `H<sub>2</sub>O` does not, and `(:u[x])` breaks after the bracket. HTML is
 * both standard Markdown and what comark parses most reliably, so that is what
 * these emit.
 *
 * Because these renderers turn document text into markup, everything that goes
 * inside a tag is escaped and every attribute value is checked against what it
 * is allowed to be. Content arrives from imports as much as from typing, and a
 * colour lifted out of someone else's stylesheet is not a colour until it has
 * been looked at.
 */

/** A mark's attributes, as the renderer receives them. */
type MarkAttributes = Record<string, unknown>

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
}

/** The text a mark wraps, with any nested marks already rendered. */
function inner(node: JSONContent, helpers: MarkdownRendererHelpers): string {
  // Text is being placed inside a tag, so it stops being text unless escaped.
  if (node.text !== undefined) return escapeHtml(node.text)
  // The children, never the node itself: passing the node back re-enters this
  // renderer and recurses until the stack gives out.
  return node.content ? helpers.renderChildren(node.content) : ""
}

function attribute(name: string, value: string): string {
  if (!value) return ""
  return ` ${name}="${escapeHtml(value)}"`
}

/** `#abc`, `#aabbcc`, `rgb(…)`, or a plain colour name. */
const COLOUR = /^(#[0-9a-f]{3,8}|rgba?\(\s*[\d.,\s%/]+\)|[a-z]+)$/i
/** `14px`, `1.5rem`, `120%`. */
const LENGTH = /^\d+(\.\d+)?(px|em|rem|pt|%)$/
/** A bare number, as line height takes. */
const NUMBER = /^\d+(\.\d+)?$/
/** Quoted or bare family names, commas and spaces — nothing that opens a call. */
const FONT_FAMILY = /^[\w\s,'"-]+$/

/** Returns the value only if it is what it claims to be. */
function checked(value: unknown, pattern: RegExp): string {
  const text = typeof value === "string" ? value.trim() : ""
  return text && pattern.test(text) ? text : ""
}

/** Renders a mark as an inline HTML tag. */
export function asInlineHtml(
  tag: string,
  attributes: (attrs: MarkAttributes) => string = () => ""
) {
  return function renderMarkdown(
    node: JSONContent,
    helpers: MarkdownRendererHelpers
  ): string {
    return `<${tag}${attributes(node.attrs ?? {})}>${inner(node, helpers)}</${tag}>`
  }
}

export const underlineMarkdown = asInlineHtml("u")

export const highlightMarkdown = asInlineHtml("mark", (attrs) =>
  attribute("data-color", checked(attrs.color, COLOUR))
)

export const subscriptMarkdown = asInlineHtml("sub")
export const superscriptMarkdown = asInlineHtml("sup")

/**
 * Colour, size and typeface all live on one mark, so they are written as one
 * `<span>` carrying whichever of them is set — and only in a form that cannot
 * close the declaration and start another.
 */
export const textStyleMarkdown = asInlineHtml("span", (attrs) => {
  const declarations: Array<[string, string]> = [
    ["color", checked(attrs.color, COLOUR)],
    ["background-color", checked(attrs.backgroundColor, COLOUR)],
    ["font-size", checked(attrs.fontSize, LENGTH)],
    ["font-family", checked(attrs.fontFamily, FONT_FAMILY)],
    ["line-height", checked(attrs.lineHeight, NUMBER) || checked(attrs.lineHeight, LENGTH)],
  ]
  const style = declarations
    .filter(([, value]) => value)
    .map(([property, value]) => `${property}: ${value}`)
    .join("; ")
  return attribute("style", style)
})

const ALIGNMENTS = new Set(["center", "right", "justify"])

/** The alignment on a block, if it is one worth writing down. */
function alignmentOf(node: JSONContent): string {
  const align = node.attrs?.textAlign
  return typeof align === "string" && ALIGNMENTS.has(align) ? align : ""
}

/**
 * Alignment is an attribute of the block, not a mark, so a centred paragraph
 * becomes a `<p style="text-align: center">`. Left is the default and is
 * written as ordinary Markdown.
 *
 * No trailing newlines from either of these: the serializer puts the blank
 * line between blocks itself, and adding another turns every paragraph into
 * two — half of them empty.
 */
export function alignedParagraphMarkdown(
  node: JSONContent,
  helpers: MarkdownRendererHelpers
): string {
  const body = node.content ? helpers.renderChildren(node.content) : ""
  const align = alignmentOf(node)
  return align ? `<p style="text-align: ${align}">${body}</p>` : body
}

/** As above, but keeping the `##` that makes a heading a heading. */
export function alignedHeadingMarkdown(
  node: JSONContent,
  helpers: MarkdownRendererHelpers
): string {
  const body = node.content ? helpers.renderChildren(node.content) : ""
  const level = Math.min(Math.max(Number(node.attrs?.level) || 1, 1), 6)
  const align = alignmentOf(node)
  if (align) return `<h${level} style="text-align: ${align}">${body}</h${level}>`
  return `${"#".repeat(level)} ${body}`
}
