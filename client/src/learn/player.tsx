import { watching } from "@/learn/api"

/**
 * A lesson's video.
 *
 * Played from the API rather than from the uploads address: what a student may
 * watch is decided per request, and a picture on a page being public is not
 * the same as a course's video being public.
 */
export function Player({ lessonId }: { lessonId: string }) {
  return (
    <video
      key={lessonId}
      controls
      controlsList="nodownload"
      className="w-full rounded-xl border border-border bg-black"
      src={watching(lessonId)}
    />
  )
}
