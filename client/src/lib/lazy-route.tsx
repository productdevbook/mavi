import * as React from "react"

import { DashboardLoading } from "@/components/dashboard/dashboard-page"

type ComponentModule<Props extends object> = {
  default: React.ComponentType<Props>
}

/**
 * Keeps feature code out of the initial panel bundle. The route module stays
 * in the generated route tree, but the feature chunk is requested only when
 * the route is actually rendered (and the plugin gate has allowed it).
 */
export function lazyRoute<Props extends object>(
  load: () => Promise<ComponentModule<Props>>,
): React.FC<Props> {
  const Component = React.lazy(load)

  return function LazyRoute(props: Props) {
    return (
      <React.Suspense fallback={<DashboardLoading />}>
        <Component {...props} />
      </React.Suspense>
    )
  }
}
