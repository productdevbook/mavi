/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { StudentsPage } from "@/features/learning/students-page"

export const Route = createFileRoute("/dashboard/students")({
  component: StudentsPage,
})
