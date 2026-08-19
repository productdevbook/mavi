/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { CoursePage } from "@/features/learning/course-page"

export const Route = createFileRoute("/dashboard/courses/$courseId")({
  component: CourseRoute,
})

function CourseRoute() {
  const { courseId } = Route.useParams()
  return <CoursePage courseId={courseId} />
}
