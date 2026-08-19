import { api } from "@/lib/api"
import type { File as Media, FileVisibility } from "@api"

/** Upload bytes through the canonical media contract. */
export async function upload(
  file: File,
  visibility: FileVisibility = "public"
): Promise<Media> {
  return api("media.files.upload", {
    query: { name: file.name, visibility },
    body: file,
  })
}
