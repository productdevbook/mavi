import * as React from "react"
import {
  Link,
  useMatchRoute,
  useNavigate,
  useRouteContext,
} from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"

import { calledIn } from "@/lib/kind-name"
import { ReportAProblem } from "@/components/report-a-problem"
import { api } from "@/lib/v1"
import { capabilityOf, usePermissions } from "@/lib/permissions"
import {
  BookOpen,
  Boxes,
  Code2,
  Database,
  Film,
  FolderTree,
  GraduationCap,
  KanbanSquare,
  Shapes,
  Globe,
  Image,
  Inbox,
  Mails,
  FileText,
  Gauge,
  Palette,
  ShieldCheck,
  LayoutDashboard,
  LogOut,
  MessageSquareWarning,
  Plug,
  Receipt,
  Rocket,
  Settings,
  Tag,
  Tags,
  Users,
  UsersRound,
  ScrollText,
  Trash2,
  Workflow,
} from "lucide-react"

import { applySurface, surfaceMark } from "@/lib/surface"
import { useContentTypes } from "@/lib/use-content-types"
import { useBoards } from "@/lib/use-boards"
import { WideSurfaceProvider } from "@/components/wide-surface"
import { useWideSurface } from "@/lib/wide-surface"
import { Button } from "@/components/ui/button"
import { ModeToggle } from "@/components/mode-toggle"
import { LocaleToggle } from "@/components/locale-toggle"
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
  SidebarTrigger,
} from "@/components/ui/sidebar"

/**
 * The panel, in two halves.
 *
 * The header carries what is true wherever you are — which site this is, who
 * you are, writing a new post, signing out. The sidebar carries the things
 * you go *into*: the editors for posts, media, people and the rest. Keeping
 * them apart is what stops the top of the screen growing a new link every
 * time the CMS learns to do something else.
 */
export function DashboardShell({ children }: { children: React.ReactNode }) {
  return (
    <WideSurfaceProvider>
      <Shell>{children}</Shell>
    </WideSurfaceProvider>
  )
}

function Shell({ children }: { children: React.ReactNode }) {
  const { t, i18n } = useLingui()
  const { can } = usePermissions()
  const navigate = useNavigate()
  const matchRoute = useMatchRoute()
  const { user, site } = useRouteContext({ from: "/dashboard" })
  const { types, find } = useContentTypes()
  const boards = useBoards()
  // A canvas is not a paragraph. See wide-surface.tsx.
  const wide = useWideSurface()

  // Which kind this button writes depends on which list is on screen. It used
  // to be a post wherever you stood, so pressing it on the courses page gave
  // you a blog post with none of a course's fields on it.
  const showing = matchRoute({ to: "/dashboard/content/$kind", fuzzy: false })
  const writing = (showing ? find(showing.kind) : undefined) ?? {
    key: "post",
    name: t`post`,
  }

  const name = site ?? undefined
  React.useEffect(() => {
    applySurface(name)
  }, [name])

  // Everything the site added itself. Posts keep the first place and their own
  // address, which is where every link already goes. Lessons are left out:
  // they belong to a course and are reached through it, and a flat list of
  // every lesson on the site is out of order and out of context.
  const own = types.filter((one) => one.key !== "post" && one.key !== "page")

  const groups = [
        {
          label: t`Content`,
          links: [
            { to: "/dashboard", label: t`Overview`, icon: LayoutDashboard },
            { to: "/dashboard/visitors", label: t`Visitors`, icon: Users },
            {
              to: "/dashboard/performance",
              label: t`Performance`,
              icon: Gauge,
            },
            { to: "/dashboard/content/post", label: t`Posts`, icon: FileText },
            { to: "/dashboard/media", label: t`Media`, icon: Image },
            {
              to: "/dashboard/categories",
              label: t`Categories`,
              icon: FolderTree,
            },
            { to: "/dashboard/tags", label: t`Tags`, icon: Tags },
            { to: "/dashboard/forms", label: t`Forms`, icon: Inbox },
            { to: "/dashboard/mail", label: t`Mail`, icon: Mails },
            { to: "/dashboard/flows", label: t`Flows`, icon: Workflow },
            { to: "/dashboard/trash", label: t`Bin`, icon: Trash2 },
          ],
        },
        // The boards this site made, under their own names. A site with none sees
        // nothing about them at all: the first version of this put "Applications"
        // in every site's menu, nursery and paper merchant included.
        ...(boards.length > 0
          ? [
              {
                label: t`Working through`,
                links: boards.map((one) => ({
                  to: `/dashboard/boards/${one.id}`,
                  label: one.name,
                  icon: KanbanSquare,
                })),
              },
            ]
          : []),
        // The site's own kinds, together and named as such. Mixed in among Media
        // and Categories they read as things this panel came with, and a site with
        // four of them had four identical rows told apart by reading.
        ...(own.length > 0
          ? [
              {
                label: t`This site's own`,
                links: own.map((one) => ({
                  to: `/dashboard/content/${one.key}`,
                  label: calledIn(one, i18n.locale, true),
                  icon: Shapes,
                })),
              },
            ]
          : []),
        {
          label: t`Teaching`,
          links: [
            {
              to: "/dashboard/teaching/start",
              label: t`Start a course`,
              icon: BookOpen,
            },
            { to: "/dashboard/videos", label: t`Videos`, icon: Film },
            {
              to: "/dashboard/students",
              label: t`Students`,
              icon: GraduationCap,
            },
          ],
        },
        {
          label: t`Shop`,
          links: [
            { to: "/dashboard/products", label: t`Products`, icon: Boxes },
            { to: "/dashboard/orders", label: t`Orders`, icon: Receipt },
            { to: "/dashboard/coupons", label: t`Discount codes`, icon: Tag },
            { to: "/dashboard/letters", label: t`Letters`, icon: Mails },
          ],
        },
        {
          label: t`This site`,
          links: [
            {
              to: "/dashboard/content-types",
              label: t`Content types`,
              icon: Shapes,
            },
            { to: "/dashboard/boards", label: t`Boards`, icon: KanbanSquare },
            { to: "/dashboard/languages", label: t`Languages`, icon: Globe },
            { to: "/dashboard/plugins", label: t`Plugins`, icon: Plug },
            { to: "/dashboard/users", label: t`People`, icon: UsersRound },
            { to: "/dashboard/roles", label: t`Roles`, icon: ShieldCheck },
            { to: "/dashboard/audit", label: t`Record`, icon: ScrollText },
            { to: "/dashboard/api", label: t`API`, icon: Code2 },
            { to: "/dashboard/design", label: t`Design`, icon: Palette },
            { to: "/dashboard/settings", label: t`This site`, icon: Settings },
            { to: "/dashboard/usage", label: t`Usage`, icon: Database },
            { to: "/dashboard/publish", label: t`Publish`, icon: Rocket },
          ],
        },
      ]

  // Which capability a menu link belongs to, so the sidebar shows only what
  // this person's role may open. Links with no capability (the overview, the
  // customer's own payments) are always shown; the server still gates each
  // request either way.

  const visibleGroups = groups
    .map((group) => ({
      ...group,
      links: group.links.filter((link) => {
        const capability = capabilityOf(link.to)
        return capability === null || can(capability)
      }),
    }))
    .filter((group) => group.links.length > 0)

  return (
    <SidebarProvider>
      <Sidebar collapsible="icon">
        <SidebarHeader>
          <div className="flex items-center gap-2 px-2 py-1.5">
            <span className="surface-mark flex size-7 shrink-0 items-center justify-center rounded-lg text-sm font-bold text-white">
              {surfaceMark(name)}
            </span>
            <div className="min-w-0 group-data-[collapsible=icon]:hidden">
              <p className="truncate text-sm font-semibold">
                {name ?? "Mavi CMS"}
              </p>
            </div>
          </div>
        </SidebarHeader>

        <SidebarContent>
          {visibleGroups.map((group) => (
            <SidebarGroup key={group.label}>
              <SidebarGroupLabel>{group.label}</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  {group.links.map((link) => (
                    <SidebarMenuItem key={link.to}>
                      <SidebarMenuButton
                        isActive={
                          matchRoute({
                            to: link.to,
                            fuzzy: link.to !== "/dashboard",
                          }) !== false
                        }
                        tooltip={link.label}
                        render={<Link to={link.to} />}
                      >
                        <link.icon />
                        <span>{link.label}</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          ))}
        </SidebarContent>

        {/* Last thing in the sidebar, on every screen. Somebody whose site
            feels slow should not have to go looking for whether it is them. */}

        <SidebarRail />
      </Sidebar>

      <SidebarInset className="surface-bar bg-background">
        <header className="flex items-center gap-2 border-b border-border px-4 py-2">
          <SidebarTrigger />
          <span className="text-sm font-medium">{name ?? "Mavi CMS"}</span>

          <div className="flex-1" />

          <span className="hidden text-sm text-muted-foreground sm:inline">
            {user.name}
          </span>
          <Button
            size="sm"
            onClick={() =>
              navigate({
                to: "/editor/new",
                search: {
                  kind: writing.key,
                  locale: undefined,
                  translationOf: undefined,
                },
              })
            }
          >
            {t`New ${writing.name}`}
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
            onClick={() => {
              void api("DELETE /api/sessions").finally(() => navigate({ to: "/login" }))
            }}
          >
            <LogOut />
          </Button>
        </header>

        {/* SidebarInset is the <main>; this is only what centres the page —
            for the pages that are read. A work surface says so and gets the
            width the screen actually has. */}
        <div
          className={
            wide
              ? "w-full flex-1 px-6 py-6"
              : "mx-auto w-full max-w-5xl flex-1 px-6 py-8"
          }
        >
          {children}
        </div>
      </SidebarInset>
    </SidebarProvider>
  )
}
