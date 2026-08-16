import type * as React from "react"
import { useLocation } from "@tanstack/react-router"
import { Trans } from "@lingui/react/macro"
import { Lock } from "lucide-react"

import { capabilityOf, usePermissions } from "@/lib/permissions"

/**
 * A screen a role cannot open is not drawn.
 *
 * The menu hid what a role could not use and every screen still rendered in
 * full to anybody who typed its address: what was hidden was the door rather
 * than the room. The API refuses each request either way — this is the panel
 * being honest about it rather than showing somebody a screen whose every
 * button answers no.
 */
export function Allowed({ children }: { children: React.ReactNode }) {
  const { pathname } = useLocation()
  const { can, ready } = usePermissions()

  const capability = capabilityOf(pathname)

  // Until the grants arrive nothing is drawn: a flash of a screen somebody may
  // not open is the same mistake, one frame long.
  if (!ready) {
    return null
  }

  if (capability && !can(capability)) {
    return (
      <div className="mx-auto flex max-w-md flex-col items-center gap-2 py-24 text-center">
        <Lock className="size-8 text-muted-foreground" />
        <h1 className="font-medium">
          <Trans>This is not yours to open</Trans>
        </h1>
        <p className="text-sm text-muted-foreground">
          <Trans>
            Your role does not reach this screen. Whoever runs the site can
            change that under People.
          </Trans>
        </p>
      </div>
    )
  }

  return children
}
