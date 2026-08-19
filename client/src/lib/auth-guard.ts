import { redirect } from "@tanstack/react-router"

import { api, ApiRefused } from "@/lib/api"
import { whoAmI, type Me } from "@/lib/auth"

export type { Me }

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
  const user = await whoAmI().catch(() => {
    throw redirect({ to: "/login", search: { redirect: currentHref } })
  })

  const site = await api("settings.read")
    .then((settings) => settings.name || null)
    .catch((error: unknown) => {
      // Site settings are permissioned. The session itself is the auth guard;
      // a person who cannot view settings still gets the panel with its
      // neutral site title.
      if (error instanceof ApiRefused && error.code === "forbidden") {
        return null
      }
      throw error
    })

  return { user, site }
}
