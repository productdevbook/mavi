/**
 * The parts of the language setting that carry no catalogue with them.
 *
 * Split out so the member area can ask which language somebody is in without
 * importing `@/i18n`, which loads both catalogues at module scope — right for
 * a panel with a language menu, and a few hundred kilobytes of somebody else's
 * language on a student's phone.
 */
export const locales = {
  tr: "Türkçe",
  en: "English",
} as const

export type Locale = keyof typeof locales

export const defaultLocale: Locale = "tr"

export const STORAGE_KEY = "mavicms:locale"

export function isLocale(value: string | null): value is Locale {
  return value === "tr" || value === "en"
}

export function getStoredLocale(): Locale {
  const stored = window.localStorage.getItem(STORAGE_KEY)
  return isLocale(stored) ? stored : defaultLocale
}
