/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const StudentsPage = lazyRoute(() =>
  import("@/features/learning/students-page").then(({ StudentsPage }) => ({
    default: StudentsPage,
  })),
)

export const Route = createFileRoute("/dashboard/students")({
  component: () => <StudentsPage />,
})
