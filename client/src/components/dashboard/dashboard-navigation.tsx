import { Link, useMatchRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"

import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
} from "@/components/ui/sidebar"
import {
  type DashboardNavGroup,
  visibleDashboardNavigation,
} from "@/lib/dashboard-navigation"
import { surfaceMark } from "@/lib/surface"
import { usePermissions } from "@/lib/permissions"

interface DashboardNavigationProps {
  siteName?: string
  groups: DashboardNavGroup[]
}

/** The site identity and permission-aware destination tree. */
export function DashboardNavigation({
  siteName,
  groups,
}: DashboardNavigationProps) {
  const { t } = useLingui()
  const { can } = usePermissions()
  const matchRoute = useMatchRoute()
  const visibleGroups = visibleDashboardNavigation(groups, can)

  return (
    <Sidebar collapsible="icon">
      <SidebarHeader>
        <div className="flex items-center gap-2 px-2 py-1.5">
          <span className="surface-mark flex size-7 shrink-0 items-center justify-center rounded-lg text-sm font-bold text-white">
            {surfaceMark(siteName)}
          </span>
          <div className="min-w-0 group-data-[collapsible=icon]:hidden">
            <p className="truncate text-sm font-semibold">
              {siteName ?? t`Mavi CMS`}
            </p>
            <p className="truncate text-xs text-muted-foreground">
              {t`Site workspace`}
            </p>
          </div>
        </div>
      </SidebarHeader>

      <SidebarContent>
        {visibleGroups.map((group) => (
          <SidebarGroup key={group.id}>
            <SidebarGroupLabel>{group.label}</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {group.items.map((item) => (
                  <SidebarMenuItem key={item.id}>
                    <SidebarMenuButton
                      isActive={
                        matchRoute({
                          to: item.to,
                          fuzzy: item.to !== "/dashboard",
                        }) !== false
                      }
                      tooltip={item.label}
                      render={<Link to={item.to} />}
                    >
                      <item.icon />
                      <span>{item.label}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        ))}
      </SidebarContent>

      <SidebarRail />
    </Sidebar>
  )
}
