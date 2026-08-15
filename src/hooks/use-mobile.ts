import * as React from "react"

const MOBILE_BREAKPOINT = 768

const queries = new Map<number, MediaQueryList>()

function query(width: number) {
  let mql = queries.get(width)
  if (!mql) {
    mql = window.matchMedia(`(max-width: ${width - 1}px)`)
    queries.set(width, mql)
  }
  return mql
}

/// True while the viewport is narrower than `width`, one of Tailwind's
/// breakpoints. Matches what a `md:`/`xl:` class does, for the cases where the
/// difference is a different component rather than a different style.
export function useNarrowerThan(width: number) {
  const subscribe = React.useCallback(
    (onChange: () => void) => {
      const mql = query(width)
      mql.addEventListener("change", onChange)
      return () => mql.removeEventListener("change", onChange)
    },
    [width]
  )
  return React.useSyncExternalStore(
    subscribe,
    () => query(width).matches,
    // Rendered on a server, where there is no window and so no small screen.
    () => false
  )
}

export function useIsMobile() {
  return useNarrowerThan(MOBILE_BREAKPOINT)
}
