import { i18n } from "@lingui/core"

import { messages as trMessages } from "@/locales/tr/messages.po"
import { messages as enMessages } from "@/locales/en/messages.po"
import {
  STORAGE_KEY,
  defaultLocale,
  getStoredLocale,
  locales,
  type Locale,
} from "@/i18n.shared"

// The panel loads both, because switching language from the menu should happen
// rather than fetch. The member area does not; see `@/learn/i18n`.
i18n.load({ tr: trMessages, en: enMessages })

export function setLocale(locale: Locale) {
  window.localStorage.setItem(STORAGE_KEY, locale)
  document.documentElement.lang = locale
  i18n.activate(locale)
}

document.documentElement.lang = getStoredLocale()
i18n.activate(getStoredLocale())

export { i18n, locales, defaultLocale, getStoredLocale }
export type { Locale }
