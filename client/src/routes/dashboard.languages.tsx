/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const LanguagesPage = lazyRoute(() =>
  import("@/features/content/languages-page").then(({ LanguagesPage }) => ({
    default: LanguagesPage,
  })),
)

export const Route = createFileRoute("/dashboard/languages")({
  component: () => <LanguagesPage />,
})
