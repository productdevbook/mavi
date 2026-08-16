const WORDS_PER_MINUTE = 200

export function readingTimeMinutes(words: number): number {
  return Math.max(1, Math.round(words / WORDS_PER_MINUTE))
}

/**
 * Only for in-page anchors, where an answer is needed synchronously as the
 * document changes. Post addresses come from `GET /slug` so that the server
 * is the single place that decides what a slug looks like.
 *
 * Loosely mirrors `slugify` in backend/api/src/slug.rs — letters and digits are kept
 * in any script. Stripping non-ASCII would empty out Japanese or Cyrillic
 * titles, and a hardcoded transliteration map can't be right for every
 * language at once (Turkish wants ü→u, German wants ü→ue).
 */
export function slugify(value: string): string {
  return (
    value
      .trim()
      .toLowerCase()
      // Dropped rather than treated as a separator: lowercasing Turkish "İ"
      // yields "i" plus a combining dot, which would split "İstanbul" into
      // "i-stanbul".
      .replace(/\p{M}+/gu, "")
      .replace(/[^\p{L}\p{N}]+/gu, "-")
      .replace(/^-+|-+$/g, "")
  )
}

export function downloadFile(filename: string, content: string, mime: string) {
  const blob = new Blob([content], { type: `${mime};charset=utf-8` })
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement("a")
  anchor.href = url
  anchor.download = filename
  anchor.click()
  URL.revokeObjectURL(url)
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`
}

export const isMac =
  typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform)

export function shortcut(keys: string): string {
  return isMac
    ? keys.replace(/Mod/g, "⌘").replace(/Alt/g, "⌥").replace(/Shift/g, "⇧")
    : keys.replace(/Mod/g, "Ctrl")
}
