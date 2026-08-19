/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { BoardsPage } from "@/features/boards/boards-page"

export const Route = createFileRoute("/dashboard/boards")({
  component: BoardsPage,
})
