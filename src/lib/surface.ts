/**
 * Which of the three things you are looking at.
 *
 * They are the same program on the same address family, and telling them apart
 * by reading the URL is exactly what nobody does. So each one says which it is
 * in the tab title, in the browser's icon, and in the colour along the top —
 * three places you see without looking.
 */

export type SurfaceKind = "server" | "site"

interface Surface {
  kind: SurfaceKind
  /** The agency's or the site's own name, when there is one to show. */
  name?: string
}

const LOOK: Record<SurfaceKind, { colour: string; letter: string; label: string }> = {
  // Slate: the machine itself.
  server: { colour: "#475569", letter: "S", label: "Sunucu" },
  // Blue: a site, which is what most people are here for most of the time.
  site: { colour: "#2563eb", letter: "M", label: "Site" },
}

/**
 * The letter on the mark and in the tab icon.
 *
 * A site's or an agency's own initial when it has a name, so that ten open
 * tabs of the same panel are ten different icons rather than ten copies of
 * one. The kind's own letter is the fallback, and it is what the server
 * always gets: the machine has no name of its own to take an initial from.
 */
export function surfaceMark(kind: SurfaceKind, name?: string): string {
  const first = name?.trim().match(/\p{L}|\p{N}/u)?.[0]
  return (first ?? LOOK[kind].letter).toLocaleUpperCase("tr")
}

/** A favicon with the right letter and colour, without a request for it. */
function icon(kind: SurfaceKind, name?: string): string {
  const { colour } = LOOK[kind]
  const letter = surfaceMark(kind, name)
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
<rect width="64" height="64" rx="14" fill="${colour}"/>
<text x="32" y="45" font-family="system-ui,sans-serif" font-size="38" font-weight="700"
      fill="#fff" text-anchor="middle">${letter}</text></svg>`

  return `data:image/svg+xml,${encodeURIComponent(svg)}`
}

/**
 * Names the tab, colours the icon, and marks the document so the stylesheet can
 * colour the header to match.
 */
export function applySurface({ kind, name }: Surface): void {
  const { label } = LOOK[kind]
  document.title = name ? `${name} · ${label}` : `Mavi CMS · ${label}`
  document.documentElement.dataset.surface = kind

  const link =
    document.querySelector<HTMLLinkElement>("link[rel='icon']") ??
    document.head.appendChild(Object.assign(document.createElement("link"), { rel: "icon" }))
  link.type = "image/svg+xml"
  link.href = icon(kind, name)
}

export function surfaceLabel(kind: SurfaceKind): string {
  return LOOK[kind].label
}
