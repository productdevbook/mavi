/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { LettersPage } from "@/features/mail/letters-page"

export const Route = createFileRoute("/dashboard/letters")({
  component: LettersPage,
})
