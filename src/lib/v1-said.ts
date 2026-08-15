/**
 * Turning what the API refused into something a person reads.
 *
 * The API answers with a key rather than a sentence, so that what somebody is
 * told can be said in their own language. This is where the panel's own wording
 * for those keys lives; anything it has no wording for falls back to the
 * English the API sent, which is better than an empty box.
 *
 * These wordings stay in the panel rather than moving to the server: the API
 * already sends a key and named arguments, so the server could word every one
 * of these itself, but only the screen showing a refusal knows how terse or
 * how gentle it should read next to whatever else is on it, and that
 * judgement is worth keeping close to the reader. What was wrong here was
 * never where the wording lived — it is that it opted out of translation by
 * being a string literal instead of a Lingui message.
 */

import { t } from "@lingui/core/macro"

import { Refused } from "./v1"

/**
 * What the panel says for a key. Only the ones a person actually meets: a
 * wording for every key the API has would be forty sentences nobody reads, and
 * the English fallback covers the rest until somebody meets one.
 */
const WORDING: Record<string, (named: Record<string, string>) => string> = {
  not_sign_we_know: () => t`That is not a sign-in we know.`,
  second_factor_required: () => t`Enter the six digits from your authenticator.`,
  those_digits_not_ones_app_showing: () =>
    t`Those are not the digits the app is showing.`,
  password_at_least_twelve_characters: () =>
    t`A password is at least twelve characters.`,
  change_asked_for_from_somewhere_else: () =>
    t`That request did not come from this site. Reload the page and try again.`,
  that_site_has_no_room_left: (named) =>
    t`This site has no room left: it is using ${bytes(named.used)} of ${bytes(named.limit)}.`,
  that_kind_of_thing_has_no_such_field: (named) =>
    t`This kind of thing has no field called ${named.field}.`,
  that_kind_of_thing_wants_that_field: (named) => t`${named.field} is wanted.`,
  that_is_not_what_that_field_holds: (named) =>
    t`That is not what ${named.field} holds.`,
  nothing_can_be_asked_about_that_field: (named) =>
    t`Nothing was declared called ${named.field}, so nothing can be asked about it.`,
  that_letter_cannot_name_that: (named) =>
    t`This letter has nothing to put where it says {${named.name}}.`,
  this_machine_is_already_set_up: () =>
    t`This machine already has somebody running it.`,
  this_machine_is_not_set_up_yet: () =>
    t`This machine has nobody running it yet. Set it up first.`,
  that_site_already_answers_to_that_name: () =>
    t`A site already answers to that name.`,
  a_language_is_two_letters_and_a_place: () =>
    t`A language is two letters, and a place after them where there is one: en, or en-GB.`,
  this_site_already_writes_in_that: () => t`This site already writes in that.`,
  a_site_writes_in_one_language_by_default: () =>
    t`A site writes in one language unless a post says otherwise, so there has to be one.`,
  something_is_written_in_that_language: (named) =>
    t`${named.posts} things are written in that language.`,
}

/** What to show somebody when a call was refused. */
export function said(why: unknown): string {
  if (!(why instanceof Refused)) {
    return t`Something went wrong.`
  }

  // Nothing about a five hundred is a person's business: what it says is
  // already "something went wrong", and repeating the detail helps nobody.
  if (!why.expected) {
    return t`Something went wrong. It has been written down.`
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
