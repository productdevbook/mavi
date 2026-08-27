/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const LettersPage = lazyRoute(() =>
  import("@/features/mail/letters-page").then(({ LettersPage }) => ({
    default: LettersPage,
  })),
)

export const Route = createFileRoute("/dashboard/letters")({
  component: () => <LettersPage />,
})
