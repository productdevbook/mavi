/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute, useNavigate } from "@tanstack/react-router"

import { BoardPage } from "@/features/boards/board-page"

export const Route = createFileRoute("/dashboard/boards_/$boardId")({
  component: BoardRoute,
})

function BoardRoute() {
  const navigate = useNavigate()
  const { boardId } = Route.useParams()

  return (
    <BoardPage
      boardId={boardId}
      onBack={() => void navigate({ to: "/dashboard/boards" })}
    />
  )
}
