import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { useNavigate } from "@tanstack/react-router"
import { LogOut, MessageSquareWarning } from "lucide-react"

import { LocaleToggle } from "@/components/locale-toggle"
import { ModeToggle } from "@/components/mode-toggle"
import { ReportAProblem } from "@/components/report-a-problem"
import { Button } from "@/components/ui/button"
import { SidebarTrigger } from "@/components/ui/sidebar"
import { signOut as authSignOut } from "@/lib/server-next-auth"

interface DashboardHeaderProps {
  siteName?: string
  userName?: string
  writing: { key: string; name: string }
}

/** Global controls only: page-specific navigation stays in the sidebar. */
export function DashboardHeader({
  siteName,
  userName,
  writing,
}: DashboardHeaderProps) {
  const { t } = useLingui()
  const navigate = useNavigate()

  const signOut = React.useCallback(() => {
    void authSignOut().finally(() => navigate({ to: "/login" }))
  }, [navigate])

  return (
    <header className="flex min-h-14 items-center gap-2 border-b border-border px-3 sm:px-4">
      <SidebarTrigger />

      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium">
          {siteName ?? t`Mavi CMS`}
        </p>
        <p className="hidden truncate text-xs text-muted-foreground sm:block">
          {userName ?? t`Signed in`}
        </p>
      </div>

      <Button
        size="sm"
        onClick={() =>
          void navigate({
            to: "/editor/new",
            search: {
              kind: writing.key,
              locale: undefined,
              translationOf: undefined,
            },
          })
        }
      >
        <span className="hidden sm:inline">{t`New ${writing.name}`}</span>
        <span className="sm:hidden">{t`New`}</span>
      </Button>

      <ReportAProblem
        trigger={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t`Report a problem`}
            title={t`Report a problem`}
          >
            <MessageSquareWarning />
          </Button>
        }
      />
      <LocaleToggle />
      <ModeToggle />
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={t`Sign out`}
        title={t`Sign out`}
        onClick={signOut}
      >
        <LogOut />
      </Button>
    </header>
  )
}
