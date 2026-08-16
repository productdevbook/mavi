import "../report"
import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { I18nProvider } from "@lingui/react"

import "@/index.css"
import { activate, i18n } from "@/learn/i18n"
import { App } from "@/learn/app"
import { reloadOnStaleChunk } from "@/lib/stale-chunk"

/**
 * Where a student watches.
 *
 * Its own entry rather than a route in the panel, because the panel is at
 * `/admin`, and somebody opening a course they paid for should be on the site
 * they bought it from rather than at the administrator's address reading the
 * name of the CMS.
 */
async function start() {
  reloadOnStaleChunk()
  await activate()

  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <I18nProvider i18n={i18n}>
        <App />
      </I18nProvider>
    </StrictMode>
  )
}

void start()
