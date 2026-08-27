/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const BoardsPage = lazyRoute(() =>
  import("@/features/boards/boards-page").then(({ BoardsPage }) => ({
    default: BoardsPage,
  })),
)

export const Route = createFileRoute("/dashboard/boards")({
  component: () => <BoardsPage />,
})
