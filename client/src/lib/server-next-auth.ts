import { t } from "@lingui/core/macro"

import type { CurrentSession } from "@api-next"

import {
  forgetServerNextSession,
  nextApi,
  rememberServerNextSession,
  ServerNextRefused,
  serverNextSession,
} from "@/lib/server-next"

export type Me = CurrentSession

export async function signIn(
  email: string,
  password: string,
): Promise<Me> {
  const session = await nextApi("auth.session.create", {
    body: { email, password },
  })

  rememberServerNextSession(session.token)

  try {
    return await whoAmI()
  } catch (error) {
    forgetServerNextSession()
    throw error
  }
}

export async function whoAmI(): Promise<Me> {
  try {
    return await nextApi("auth.session.current")
  } catch (error) {
    if (error instanceof ServerNextRefused && error.status === 401) {
      forgetServerNextSession()
    }
    throw error
  }
}

export async function signOut(): Promise<void> {
  if (!serverNextSession()) return

  try {
    await nextApi("auth.session.revoke")
  } finally {
    forgetServerNextSession()
  }
}

export function holds(me: Me, capability: string, action = "view"): boolean {
  return me.grants.some(
    (grant) => grant.capability === capability && grant.action === action,
  )
}

export function serverNextMessage(error: unknown): string {
  if (!(error instanceof ServerNextRefused)) {
    return t`Something went wrong. It has been written down.`
  }

  if (!error.expected) {
    return t`Something went wrong. It has been written down.`
  }

  switch (error.code) {
    case "unauthenticated":
      return t`That email or password is not correct.`
    case "conflict":
      return t`This request conflicts with the current site state.`
    case "rate_limited":
      return t`Too many attempts. Wait a little and try again.`
    case "validation":
      return error.field
        ? t`Check the ${error.field} field.`
        : t`Check the values and try again.`
    default:
      return error.message
  }
}
