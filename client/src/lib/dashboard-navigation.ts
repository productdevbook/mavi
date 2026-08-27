import type { LucideIcon } from "lucide-react"

import type { Capability } from "@/lib/permissions"
import type { PluginId } from "@/lib/plugins"
import type { ContentType } from "@/lib/use-content-types"
import type { Board } from "@api"

/** A single destination in the site panel. */
export interface DashboardNavItem {
  id: string
  to: string
  label: string
  icon: LucideIcon
  capability: Capability | null
  plugin: PluginId
}

/** A named area of the panel. Empty groups are never rendered. */
export interface DashboardNavGroup {
  id: string
  label: string
  items: DashboardNavItem[]
}

/**
 * The translated words needed by the navigation manifest.
 *
 * Keeping labels as data makes the information architecture testable without
 * mounting the sidebar. The component decides how those data are drawn; this
 * function decides where a destination belongs and what protects it.
 */
export interface DashboardNavLabels {
  home: string
  overview: string
  insights: string
  visitors: string
  performance: string
  create: string
  posts: string
  media: string
  customContent: string
  organize: string
  categories: string
  tags: string
  automate: string
  forms: string
  mail: string
  flows: string
  commerce: string
  products: string
  orders: string
  discounts: string
  letters: string
  learning: string
  startCourse: string
  videos: string
  students: string
  operations: string
  boards: string
  audit: string
  trash: string
  configure: string
  contentTypes: string
  languages: string
  people: string
  roles: string
  api: string
  design: string
  settings: string
  portability: string
  usage: string
  publish: string
  plugins: string
}

interface CreateNavigationInput {
  labels: DashboardNavLabels
  contentTypes: ContentType[]
  boards: Board[]
  locale: string
  calledIn: (kind: ContentType, locale: string, plural: boolean) => string
  icons: {
    overview: LucideIcon
    visitors: LucideIcon
    performance: LucideIcon
    posts: LucideIcon
    media: LucideIcon
    customContent: LucideIcon
    categories: LucideIcon
    tags: LucideIcon
    forms: LucideIcon
    mail: LucideIcon
    flows: LucideIcon
    products: LucideIcon
    orders: LucideIcon
    discounts: LucideIcon
    letters: LucideIcon
    startCourse: LucideIcon
    videos: LucideIcon
    students: LucideIcon
    boards: LucideIcon
    audit: LucideIcon
    trash: LucideIcon
    contentTypes: LucideIcon
    languages: LucideIcon
    people: LucideIcon
    roles: LucideIcon
    api: LucideIcon
    design: LucideIcon
    settings: LucideIcon
    portability: LucideIcon
    usage: LucideIcon
    publish: LucideIcon
    plugins: LucideIcon
  }
}

/**
 * The canonical site-panel information architecture.
 *
 * URL compatibility is deliberate: the panel may move a screen between
 * groups without making bookmarks, integrations, or open tabs stale.
 */
export function createDashboardNavigation({
  labels,
  contentTypes,
  boards,
  locale,
  calledIn,
  icons,
}: CreateNavigationInput): DashboardNavGroup[] {
  const customContent = contentTypes.filter(
    (kind) => kind.key !== "post" && kind.key !== "page"
  )

  const item = (
    id: string,
    to: string,
    label: string,
    icon: LucideIcon,
    capability: Capability | null,
    plugin: PluginId = "core",
  ): DashboardNavItem => ({ id, to, label, icon, capability, plugin })

  const boardItems = boards.map((board) =>
    item(
      `board-${board.id}`,
      `/dashboard/boards/${board.id}`,
      board.name,
      icons.boards,
      "boards",
      "boards",
    )
  )

  return [
    {
      id: "home",
      label: labels.home,
      items: [
        item("overview", "/dashboard", labels.overview, icons.overview, null),
      ],
    },
    {
      id: "insights",
      label: labels.insights,
      items: [
        item(
          "visitors",
          "/dashboard/visitors",
          labels.visitors,
          icons.visitors,
          "analytics",
          "analytics",
        ),
        item(
          "performance",
          "/dashboard/performance",
          labels.performance,
          icons.performance,
          "analytics",
          "analytics",
        ),
      ],
    },
    {
      id: "create",
      label: labels.create,
      items: [
        item(
          "posts",
          "/dashboard/content/post",
          labels.posts,
          icons.posts,
          "content",
          "writing",
        ),
        item(
          "media",
          "/dashboard/media",
          labels.media,
          icons.media,
          "media",
          "writing",
        ),
        ...customContent.map((kind) =>
          item(
            `content-${kind.key}`,
            `/dashboard/content/${kind.key}`,
            calledIn(kind, locale, true),
            icons.customContent,
            "content",
            "writing",
          )
        ),
      ],
    },
    {
      id: "organize",
      label: labels.organize,
      items: [
        item(
          "categories",
          "/dashboard/categories",
          labels.categories,
          icons.categories,
          "taxonomy",
          "writing",
        ),
        item(
          "tags",
          "/dashboard/tags",
          labels.tags,
          icons.tags,
          "taxonomy",
          "writing",
        ),
      ],
    },
    {
      id: "automate",
      label: labels.automate,
      items: [
        item(
          "forms",
          "/dashboard/forms",
          labels.forms,
          icons.forms,
          "forms",
          "forms",
        ),
        item(
          "mail",
          "/dashboard/mail",
          labels.mail,
          icons.mail,
          "mail",
          "messaging",
        ),
        item(
          "flows",
          "/dashboard/flows",
          labels.flows,
          icons.flows,
          "flows",
          "automation",
        ),
      ],
    },
    {
      id: "commerce",
      label: labels.commerce,
      items: [
        item(
          "products",
          "/dashboard/products",
          labels.products,
          icons.products,
          "shop",
          "commerce",
        ),
        item(
          "orders",
          "/dashboard/orders",
          labels.orders,
          icons.orders,
          "shop",
          "commerce",
        ),
        item(
          "discounts",
          "/dashboard/coupons",
          labels.discounts,
          icons.discounts,
          "shop",
          "commerce",
        ),
        item(
          "letters",
          "/dashboard/letters",
          labels.letters,
          icons.letters,
          "mail",
          "messaging",
        ),
      ],
    },
    {
      id: "learning",
      label: labels.learning,
      items: [
        item(
          "start-course",
          "/dashboard/teaching/start",
          labels.startCourse,
          icons.startCourse,
          "courses",
          "learning",
        ),
        item(
          "videos",
          "/dashboard/videos",
          labels.videos,
          icons.videos,
          "courses",
          "learning",
        ),
        item(
          "students",
          "/dashboard/students",
          labels.students,
          icons.students,
          "courses",
          "learning",
        ),
      ],
    },
    {
      id: "operations",
      label: labels.operations,
      items: [
        ...boardItems,
        item(
          "audit",
          "/dashboard/audit",
          labels.audit,
          icons.audit,
          "audit",
          "governance",
        ),
        item(
          "trash",
          "/dashboard/trash",
          labels.trash,
          icons.trash,
          "trash",
          "governance",
        ),
      ],
    },
    {
      id: "configure",
      label: labels.configure,
      items: [
        item(
          "content-types",
          "/dashboard/content-types",
          labels.contentTypes,
          icons.contentTypes,
          "content",
          "writing",
        ),
        item(
          "languages",
          "/dashboard/languages",
          labels.languages,
          icons.languages,
          "settings",
          "core",
        ),
        item(
          "people",
          "/dashboard/users",
          labels.people,
          icons.people,
          "people",
          "core",
        ),
        item(
          "roles",
          "/dashboard/roles",
          labels.roles,
          icons.roles,
          "people",
          "core",
        ),
        item("api", "/dashboard/api", labels.api, icons.api, "settings", "core"),
        item(
          "design",
          "/dashboard/design",
          labels.design,
          icons.design,
          "design",
          "writing",
        ),
        item(
          "settings",
          "/dashboard/settings",
          labels.settings,
          icons.settings,
          "settings",
          "core",
        ),
        item(
          "portability",
          "/dashboard/portable",
          labels.portability,
          icons.portability,
          "portable",
          "governance",
        ),
        item(
          "usage",
          "/dashboard/usage",
          labels.usage,
          icons.usage,
          "analytics",
          "analytics",
        ),
        item(
          "publish",
          "/dashboard/publish",
          labels.publish,
          icons.publish,
          "publish",
          "writing",
        ),
        item(
          "plugins",
          "/dashboard/plugins",
          labels.plugins,
          icons.plugins,
          "plugins",
          "core",
        ),
      ],
    },
  ]
}

/** Apply grants to a manifest without coupling the manifest to React. */
export function visibleDashboardNavigation(
  groups: DashboardNavGroup[],
  can: (capability: Capability) => boolean,
  activePlugins: ReadonlySet<PluginId> | null = null,
): DashboardNavGroup[] {
  return groups
    .map((group) => ({
      ...group,
      items: group.items.filter(
        (item) =>
          (activePlugins?.has(item.plugin) ?? item.plugin === "core") &&
          (item.capability === null || can(item.capability)),
      ),
    }))
    .filter((group) => group.items.length > 0)
}
