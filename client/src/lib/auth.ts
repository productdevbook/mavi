import { t } from "@lingui/core/macro"

import type { CurrentSession } from "@api"

import {
  forgetApiSession,
  api,
  rememberApiSession,
  ApiRefused,
  apiSession,
} from "@/lib/api"

export type Me = CurrentSession

export async function signIn(
  email: string,
  password: string,
): Promise<Me> {
  const session = await api("auth.session.create", {
    body: { email, password },
  })

  rememberApiSession(session.token)

  try {
    return await whoAmI()
  } catch (error) {
    forgetApiSession()
    throw error
  }
}

export async function whoAmI(): Promise<Me> {
  try {
    return await api("auth.session.current")
  } catch (error) {
    if (error instanceof ApiRefused && error.status === 401) {
      forgetApiSession()
    }
    throw error
  }
}

export async function signOut(): Promise<void> {
  if (!apiSession()) return

  try {
    await api("auth.session.revoke")
  } finally {
    forgetApiSession()
  }
}

export function holds(me: Me, capability: string, action = "view"): boolean {
  return me.grants.some(
    (grant) => grant.capability === capability && grant.action === action,
  )
}

export function apiMessage(error: unknown): string {
  if (!(error instanceof ApiRefused)) {
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
