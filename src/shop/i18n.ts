import { i18n } from "@lingui/core"

import { defaultLocale, getStoredLocale, type Locale } from "@/i18n.shared"

/**
 * One language, fetched.
 *
 * The panel loads both catalogues at import time, which is right there: an
 * administrator switches language from a menu and expects it to happen. A
 * shopper opens a basket on a phone, in one language, and never changes it —
 * so the other catalogue is a few hundred kilobytes of somebody else's
 * language downloaded before a total can be shown.
 */
export async function activate(locale: Locale = getStoredLocale()) {
  const { messages } = await (locale === "en"
    ? import("@/locales/en/messages.po")
    : import("@/locales/tr/messages.po"))

  i18n.load(locale, messages)
  i18n.activate(locale)
  document.documentElement.lang = locale
}

export { i18n, defaultLocale }
