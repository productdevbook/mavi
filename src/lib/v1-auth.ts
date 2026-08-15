/**
 * Signing in, against v1.
 *
 * Signing in, the second factor and signing out, in one place: three screens
 * ask for them and none of them should know what a session cookie is called.
 */

import { api, Refused } from "./v1"
import type { Gives } from "./v1"

export type Me = Gives<"GET /api/auth/me">
export type Session = Gives<"POST /api/auth/session">

/**
 * What a sign-in can want next.
 *
 * The API tells a wrong password and a missing second factor apart, so the
 * screen can ask for the digits rather than saying the password was wrong —
 * which is the whole reason that refusal has a code of its own.
 */
export type SigningIn =
  | { done: true; session: Session }
  | { done: false; wants: "second-factor" }

export async function signIn(
  email: string,
  password: string,
  secondFactor?: string,
): Promise<SigningIn> {
  try {
    const session = await api("POST /api/auth/session", {
      body: {
        email,
        password,
        code: secondFactor?.trim() || undefined,
      },
    })

    return { done: true, session }
  } catch (why) {
    if (why instanceof Refused && why.code === "second_factor_required") {
      return { done: false, wants: "second-factor" }
    }

    throw why
  }
}

export function whoAmI(): Promise<Me> {
  return api("GET /api/auth/me")
}

export function signOut(): Promise<void> {
  return api("DELETE /api/auth/session")
}

/** Whether somebody holds a grant, for a screen deciding what to show. */
export function holds(me: Me, grant: string): boolean {
  return me.grants.includes(grant)
}
