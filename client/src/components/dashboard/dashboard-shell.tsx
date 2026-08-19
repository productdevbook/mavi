import * as React from "react"
import { useMatchRoute, useRouteContext } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
  BookOpen,
  Boxes,
  Code2,
  Database,
  Film,
  FileText,
  FolderTree,
  GraduationCap,
  Gauge,
  Globe,
  Image,
  Inbox,
  KanbanSquare,
  LayoutDashboard,
  Mails,
  Palette,
  Receipt,
  Rocket,
  ScrollText,
  Settings,
  Shapes,
  ShieldCheck,
  Tag,
  Tags,
  Trash2,
  Users,
  UsersRound,
  Workflow,
} from "lucide-react"

import { DashboardContent } from "@/components/dashboard/dashboard-content"
import { DashboardHeader } from "@/components/dashboard/dashboard-header"
import { DashboardNavigation } from "@/components/dashboard/dashboard-navigation"
import { WideSurfaceProvider } from "@/components/wide-surface"
import { calledIn } from "@/lib/kind-name"
import {
  createDashboardNavigation,
  type DashboardNavLabels,
} from "@/lib/dashboard-navigation"
import { applySurface } from "@/lib/surface"
import { useBoards } from "@/lib/use-boards"
import { useContentTypes } from "@/lib/use-content-types"
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar"

/** The stable composition root for every authenticated site screen. */
export function DashboardShell({ children }: { children: React.ReactNode }) {
  return (
    <WideSurfaceProvider>
      <AuthenticatedShell>{children}</AuthenticatedShell>
    </WideSurfaceProvider>
  )
}

function AuthenticatedShell({ children }: { children: React.ReactNode }) {
  const { t, i18n } = useLingui()
  const matchRoute = useMatchRoute()
  const { user, site } = useRouteContext({ from: "/dashboard" })
  const { types, find } = useContentTypes()
  const boards = useBoards()

  const siteName = site ?? undefined
  React.useEffect(() => applySurface(siteName), [siteName])

  const writingMatch = matchRoute({
    to: "/dashboard/content/$kind",
    fuzzy: false,
  })
  const writing = (writingMatch ? find(writingMatch.kind) : undefined) ?? {
    key: "post",
    name: t`post`,
  }

  const labels: DashboardNavLabels = {
    home: t`Home`,
    overview: t`Overview`,
    insights: t`Insights`,
    visitors: t`Visitors`,
    performance: t`Performance`,
    create: t`Create`,
    posts: t`Posts`,
    media: t`Media`,
    customContent: t`Custom content`,
    organize: t`Organize`,
    categories: t`Categories`,
    tags: t`Tags`,
    automate: t`Automate`,
    forms: t`Forms`,
    mail: t`Mail`,
    flows: t`Flows`,
    commerce: t`Commerce`,
    products: t`Products`,
    orders: t`Orders`,
    discounts: t`Discount codes`,
    letters: t`Letters`,
    learning: t`Learning`,
    startCourse: t`Start a course`,
    videos: t`Videos`,
    students: t`Students`,
    operations: t`Operations`,
    boards: t`Boards`,
    audit: t`Record`,
    trash: t`Bin`,
    configure: t`Configure`,
    contentTypes: t`Content types`,
    languages: t`Languages`,
    people: t`People`,
    roles: t`Roles`,
    api: t`API`,
    design: t`Design`,
    settings: t`Settings`,
    portability: t`Import and export`,
    usage: t`Usage`,
    publish: t`Publish`,
  }

  const navigation = createDashboardNavigation({
    labels,
    contentTypes: types,
    boards,
    locale: i18n.locale,
    calledIn,
    icons: {
      overview: LayoutDashboard,
      visitors: Users,
      performance: Gauge,
      posts: FileText,
      media: Image,
      customContent: Shapes,
      categories: FolderTree,
      tags: Tags,
      forms: Inbox,
      mail: Mails,
      flows: Workflow,
      products: Boxes,
      orders: Receipt,
      discounts: Tag,
      letters: Mails,
      startCourse: BookOpen,
      videos: Film,
      students: GraduationCap,
      boards: KanbanSquare,
      audit: ScrollText,
      trash: Trash2,
      contentTypes: Shapes,
      languages: Globe,
      people: UsersRound,
      roles: ShieldCheck,
      api: Code2,
      design: Palette,
      settings: Settings,
      portability: Database,
      usage: Database,
      publish: Rocket,
    },
  })

  return (
    <SidebarProvider>
      <DashboardNavigation siteName={siteName} groups={navigation} />
      <SidebarInset className="surface-bar bg-background">
        <DashboardHeader
          siteName={siteName}
          userName={user.name}
          writing={writing}
        />
        <DashboardContent>{children}</DashboardContent>
      </SidebarInset>
    </SidebarProvider>
  )
}
