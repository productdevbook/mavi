/**
 * Signing in, against v1.
 *
 * Signing in, the second factor and signing out, in one place: three screens
 * ask for them and none of them should know what a session cookie is called.
 */

import { api } from "./v1"
import type { Person, Session, WayIn } from "@api"

export type { Person, Session }

export interface Me {
  id?: string
  email?: string
  name?: string
  role?: string
  grants: string[]
  site: string | null
}

/**
 * What a sign-in can want next.
 *
 * The API tells a wrong password and a missing second factor apart, so the
 * screen can ask for the digits rather than saying the password was wrong —
 * which is the whole reason that refusal has a code of its own.
 */
export type SigningIn =
  | { done: true; session: Session }
  | { done: false; wants: "second-factor"; moment: string }

export async function signIn(
  email: string,
  password: string,
  secondFactor?: string,
  moment?: string,
): Promise<SigningIn> {
  if (moment && secondFactor) {
    const session = await api("sessions.finish", {
      body: {
        moment,
        code: secondFactor.trim(),
      },
    })

    return { done: true, session }
  }

  const wayIn: WayIn = await api("sessions.begin", {
    body: {
      email,
      password,
    },
  })

  if (wayIn.finished && wayIn.token && wayIn.person) {
    return {
      done: true,
      session: {
        token: wayIn.token,
        person: wayIn.person,
      },
    }
  }

  if (wayIn.moment) {
    return {
      done: false,
      wants: "second-factor",
      moment: wayIn.moment,
    }
  }

  throw new Error("Invalid sign-in response")
}

export async function whoAmI(): Promise<Me> {
  const settings = await api("settings.read")
  return {
    grants: [],
    site: settings.name,
  }
}

export async function signOut(): Promise<void> {
  await api("sessions.end")
}

/** Whether somebody holds a grant, for a screen deciding what to show. */
export function holds(me: Me, grant: string): boolean {
  return me.grants.includes(grant)
}
