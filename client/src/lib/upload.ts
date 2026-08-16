import { Refused } from "@/lib/v1"

/**
 * Sending a file, and what came back.
 *
 * The bytes as they are, with the name alongside: what kind of file it is is
 * decided at the far end from the bytes rather than from the name, so this is
 * the one call the panel makes without the typed client.
 */
export async function upload(
  file: File,
): Promise<{ id: string; url: string; name: string }> {
  const response = await fetch(
    `/api/files?name=${encodeURIComponent(file.name)}`,
    { method: "POST", body: file },
  )

  if (!response.ok) {
    const why = await response.json().catch(() => null)

    throw new Refused(
      response.status,
      String(why?.error?.code ?? "internal"),
      why?.error?.key ?? null,
      why?.error?.named ?? {},
      String(why?.error?.message ?? response.statusText),
    )
  }

  const media = (await response.json()) as { id: string; name?: string; original_name?: string }

  return { id: media.id, name: media.name ?? media.original_name ?? file.name, url: `/uploads/${media.id}` }
}
