/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { lazyRoute } from "@/lib/lazy-route"

const MailPage = lazyRoute(() =>
  import("@/features/mail/mail-page").then(({ MailPage }) => ({
    default: MailPage,
  })),
)

export const Route = createFileRoute("/dashboard/mail")({
  component: () => <MailPage />,
})
