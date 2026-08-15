/**
 * Turning what the API refused into something a person reads.
 *
 * The API answers with a key rather than a sentence, so that what somebody is
 * told can be said in their own language. This is where the panel's own wording
 * for those keys lives; anything it has no wording for falls back to the
 * English the API sent, which is better than an empty box.
 */

import { Refused } from "./v1"

/**
 * What the panel says for a key. Only the ones a person actually meets: a
 * wording for every key the API has would be forty sentences nobody reads, and
 * the English fallback covers the rest until somebody meets one.
 */
const WORDING: Record<string, (named: Record<string, string>) => string> = {
  not_sign_we_know: () => "That is not a sign-in we know.",
  second_factor_required: () => "Enter the six digits from your authenticator.",
  those_digits_not_ones_app_showing: () =>
    "Those are not the digits the app is showing.",
  password_at_least_twelve_characters: () =>
    "A password is at least twelve characters.",
  change_asked_for_from_somewhere_else: () =>
    "That request did not come from this site. Reload the page and try again.",
  that_site_has_no_room_left: (named) =>
    `This site has no room left: it is using ${bytes(named.used)} of ${bytes(named.limit)}.`,
  that_kind_of_thing_has_no_such_field: (named) =>
    `This kind of thing has no field called ${named.field}.`,
  that_kind_of_thing_wants_that_field: (named) =>
    `${named.field} is wanted.`,
  that_is_not_what_that_field_holds: (named) =>
    `That is not what ${named.field} holds.`,
  nothing_can_be_asked_about_that_field: (named) =>
    `Nothing was declared called ${named.field}, so nothing can be asked about it.`,
  that_letter_cannot_name_that: (named) =>
    `This letter has nothing to put where it says {${named.name}}.`,
  this_machine_is_already_set_up: () => "This machine already has somebody running it.",
  that_site_already_answers_to_that_name: () => "A site already answers to that name.",
  a_language_is_two_letters_and_a_place: () =>
    "A language is two letters, and a place after them where there is one: en, or en-GB.",
  this_site_already_writes_in_that: () => "This site already writes in that.",
  a_site_writes_in_one_language_by_default: () =>
    "A site writes in one language unless a post says otherwise, so there has to be one.",
  something_is_written_in_that_language: (named) =>
    `${named.posts} things are written in that language.`,
}

/** What to show somebody when a call was refused. */
export function said(why: unknown): string {
  if (!(why instanceof Refused)) {
    return "Something went wrong."
  }

  // Nothing about a five hundred is a person's business: what it says is
  // already "something went wrong", and repeating the detail helps nobody.
  if (!why.expected) {
    return "Something went wrong. It has been written down."
  }

  const wording = why.key ? WORDING[why.key] : undefined

  return wording ? wording(why.named) : why.message
}

function bytes(said: string | undefined): string {
  const many = Number(said ?? 0)

  if (!Number.isFinite(many)) {
    return said ?? "?"
  }

  const units = ["B", "kB", "MB", "GB", "TB"]
  let left = many
  let unit = 0

  while (left >= 1000 && unit < units.length - 1) {
    left /= 1000
    unit += 1
  }

  return `${Math.round(left * 10) / 10} ${units[unit]}`
}
