/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const CoursePage = lazyRoute(() =>
  import("@/features/learning/course-page").then(({ CoursePage }) => ({
    default: CoursePage,
  })),
)

export const Route = createFileRoute("/dashboard/courses/$courseId")({
  component: CourseRoute,
})

function CourseRoute() {
  const { courseId } = Route.useParams()
  return <CoursePage courseId={courseId} />
}
