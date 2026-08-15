import { redirect } from "@tanstack/react-router"

import { api } from "@/lib/v1"
import type { Me } from "../../server/types/mavicms"

/**
 * Whoever is signed in, or the sign-in screen.
 *
 * Asked before every screen behind the panel: what comes back is also what the
 * tab is called and what the sidebar shows, so a screen that draws before this
 * would draw the wrong site's name.
 */
export async function requireAuth(currentHref: string): Promise<{
  user: Me
  /** What this site calls itself, for the tab and the header. */
  site: string | null
}> {
  const user = await api("GET /api/auth/me").catch(() => {
    throw redirect({ to: "/login", search: { redirect: currentHref } })
  })

  return { user, site: user.site || null }
}
