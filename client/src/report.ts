// Off unless the build was given a DSN — a self-hosted panel reports to
// nobody unless its operator said where at build time.
import * as Sentry from "@sentry/react"

const dsn = import.meta.env.VITE_SENTRY_DSN
if (dsn) {
  Sentry.init({
    dsn,
    release: import.meta.env.VITE_SENTRY_RELEASE || undefined,
    environment: import.meta.env.MODE,
  })
}
