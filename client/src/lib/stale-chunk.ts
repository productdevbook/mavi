/**
 * Recover a page whose chunks were deployed over while it was open.
 *
 * Every route arrives as its own file, named after a hash of itself, so a
 * deploy leaves an open tab asking for files that no longer exist — and the
 * first time it asks is when somebody changes page. Reloading picks up the new
 * page and lands on the address they were going to anyway.
 *
 * Once per address, because a reload that fails the same way is a loop, and an
 * error somebody can read beats a tab that flickers for ever.
 */
export function reloadOnStaleChunk(): void {
  window.addEventListener("vite:preloadError", (event) => {
    if (sessionStorage.getItem("reloaded-for") === location.href) return

    event.preventDefault()
    sessionStorage.setItem("reloaded-for", location.href)
    location.reload()
  })
}
