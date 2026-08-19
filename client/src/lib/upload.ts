import { nextApi } from "@/lib/server-next"
import type { File as Media, FileVisibility } from "@api-next"

/** Upload bytes through the canonical media contract. */
export async function upload(
  file: File,
  visibility: FileVisibility = "public"
): Promise<Media> {
  return nextApi("media.files.upload", {
    query: { name: file.name, visibility },
    body: file,
  })
}
