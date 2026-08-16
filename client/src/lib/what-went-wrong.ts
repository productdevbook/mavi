/**
 * The last few things that went wrong in this browser.
 *
 * Somebody reporting a broken screen can say what they saw. They cannot say
 * what the console said, and that is usually the half that names the fault —
 * so it is kept here as it happens and sent with the report rather than asked
 * for.
 *
 * A ring of twenty. A screen that is failing on every render would otherwise
 * fill memory with the same line, and twenty is more than enough to see what
 * started it.
 */
const KEEP = 20

interface WentWrong {
  at: string
  what: string
}

const seen: WentWrong[] = []

function note(what: unknown) {
  const said =
    what instanceof Error
      ? `${what.name}: ${what.message}`
      : typeof what === "string"
        ? what
        : (() => {
            try {
              return JSON.stringify(what)
            } catch {
              return String(what)
            }
          })()

  // One line, and not a long one: a stack trace pasted twenty times is a wall
  // nobody reads, and the first line is what names the fault.
  seen.push({ at: new Date().toISOString(), what: said.slice(0, 300) })
  if (seen.length > KEEP) seen.shift()
}

/**
 * Starts listening. Called once, from the panel's entry point.
 *
 * Wraps `console.error` rather than only listening for uncaught errors,
 * because React reports a render that threw by logging it and carrying on —
 * which is exactly the failure somebody would want to report.
 */
export function watchForErrors() {
  window.addEventListener("error", (event) =>
    note(event.error ?? event.message)
  )
  window.addEventListener("unhandledrejection", (event) => note(event.reason))

  const wasThere = console.error
  console.error = (...args: unknown[]) => {
    note(args[0])
    wasThere.apply(console, args as never)
  }
}

/** What to send with a report: the browser, the screen, and the last faults. */
export function whatWentWrong(): Record<string, unknown> {
  return {
    screen: window.location.pathname + window.location.search,
    browser: navigator.userAgent,
    language: navigator.language,
    // A screen that is broken at one width is often fine at another.
    viewport: `${window.innerWidth}×${window.innerHeight}`,
    at: new Date().toISOString(),
    errors: seen.map((one) => `${one.at} ${one.what}`),
  }
}
