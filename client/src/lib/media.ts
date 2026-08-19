import * as React from "react"

import { api } from "@/lib/api"
import type { File as Media, FileVisibility } from "@api"

/** The only stable URL that can be embedded in published site content. */
export function publicFileUrl(id: string): string {
  return `/public/v1/files/${encodeURIComponent(id)}`
}

/**
 * Load a private file for a panel-only preview. Object URLs are revoked on
 * replacement and unmount so a media grid cannot leak every downloaded blob.
 */
export function usePrivateFileUrl(
  file: Pick<Media, "id" | "visibility">
): string | null {
  const [url, setUrl] = React.useState<string | null>(null)

  React.useEffect(() => {
    if (file.visibility === "public") {
      setUrl(publicFileUrl(file.id))
      return
    }

    let cancelled = false
    let objectUrl: string | null = null

    setUrl(null)
    api("media.files.download", { path: { id: file.id } })
      .then((blob) => {
        if (cancelled) return
        objectUrl = URL.createObjectURL(blob)
        setUrl(objectUrl)
      })
      .catch(() => {
        if (!cancelled) setUrl(null)
      })

    return () => {
      cancelled = true
      if (objectUrl) URL.revokeObjectURL(objectUrl)
    }
  }, [file.id, file.visibility])

  return url
}

export type MediaVisibility = FileVisibility
