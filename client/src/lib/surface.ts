/**
 * What names the tab and what goes on the mark.
 *
 * Ten open tabs of the same panel used to be ten copies of one icon, so the
 * site's own initial goes in the favicon and beside the menu — something you
 * see without looking.
 */

/** The mark's colour, and the stripe along the top, are the same blue. */
const COLOUR = "#2563eb"

/**
 * The letter on the mark and in the tab icon: the site's own initial, and `M`
 * where it has not been named yet.
 */
export function surfaceMark(name?: string): string {
  const first = name?.trim().match(/\p{L}|\p{N}/u)?.[0]
  return (first ?? "M").toLocaleUpperCase("tr")
}

/** A favicon with the right letter, without a request for it. */
function icon(name?: string): string {
  const letter = surfaceMark(name)
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
<rect width="64" height="64" rx="14" fill="${COLOUR}"/>
<text x="32" y="45" font-family="system-ui,sans-serif" font-size="38" font-weight="700"
      fill="#fff" text-anchor="middle">${letter}</text></svg>`

  return `data:image/svg+xml,${encodeURIComponent(svg)}`
}

/** Names the tab and colours its icon. */
export function applySurface(name?: string): void {
  document.title = name ?? "Mavi CMS"

  const link =
    document.querySelector<HTMLLinkElement>("link[rel='icon']") ??
    document.head.appendChild(Object.assign(document.createElement("link"), { rel: "icon" }))
  link.type = "image/svg+xml"
  link.href = icon(name)
}
