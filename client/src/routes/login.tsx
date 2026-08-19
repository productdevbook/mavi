/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { LoginPage } from "@/features/auth/login-page"

export const Route = createFileRoute("/login")({
  validateSearch: (search: Record<string, unknown>): { redirect?: string } => ({
    redirect: typeof search.redirect === "string" ? search.redirect : undefined,
  }),
  component: LoginRoute,
})

function LoginRoute() {
  const { redirect } = Route.useSearch()

  return <LoginPage redirectTo={redirect} />
}
