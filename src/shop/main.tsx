import "../report"
import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { I18nProvider } from "@lingui/react"

import "@/index.css"
import { activate, i18n } from "@/shop/i18n"
import { App } from "@/shop/app"
import { reloadOnStaleChunk } from "@/lib/stale-chunk"

/**
 * Where somebody pays.
 *
 * Its own entry rather than a route in the panel, and on the shop's own
 * address rather than the administrator's — a basket at /admin is a basket
 * nobody trusts.
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
