/**
 * The client boundary for the clean `server` API.
 *
 * `@api` is generated from `server/mavi-http/contracts/mavi.ts`.
 * This file owns browser concerns only: the bearer session, request reuse,
 * cursor walking, and translating a transport failure into a stable error.
 * Domain screens must depend on operation IDs and generated shapes, never on
 * the generated client's URL or fetch details.
 */

import {
  MaviApiError,
  MaviClient,
} from "@api"
import type {
  OperationArguments,
  OperationName,
  OperationResponses,
} from "@api"

const SESSION_KEY = "mavi:session"

export class ApiRefused extends Error {
  readonly status: number
  readonly code: string
  readonly field: string | null

  constructor(
    status: number,
    code: string,
    field: string | null,
    message: string
  ) {
    super(message)
    this.name = "ApiRefused"
    this.status = status
    this.code = code
    this.field = field
  }

  get expected(): boolean {
    return this.status < 500
  }
}

/** The short-lived bearer session for this browser tab, if one exists. */
export function apiSession(): string | null {
  return window.sessionStorage.getItem(SESSION_KEY)
}

/** Store only the bearer token; the session response is not cached as data. */
export function rememberApiSession(token: string): void {
  window.sessionStorage.setItem(SESSION_KEY, token)
}

/** Revoke locally after sign-out or after the server rejects the token. */
export function forgetApiSession(): void {
  window.sessionStorage.removeItem(SESSION_KEY)
}

type Asking<Name extends OperationName> = OperationArguments[Name]

/** Call one operation described by the server canonical contract. */
export async function api<Name extends OperationName>(
  operation: Name,
  args: Asking<Name> = {} as Asking<Name>
): Promise<OperationResponses[Name]> {
  const client = new MaviClient({
    baseUrl: window.location.origin,
    token: apiSession() ?? undefined,
  })

  try {
    return await client.call(operation, args)
  } catch (error) {
    throw refused(error)
  }
}

type PageAnswer = {
  items: unknown[]
  next_cursor: string | null
}

type PagedOperation = {
  [Name in OperationName]: OperationResponses[Name] extends PageAnswer
    ? Name
    : never
}[OperationName]

/** Walk a generated cursor page with a hard bound for UI reads. */
export async function every<Name extends PagedOperation>(
  operation: Name,
  args: Asking<Name> = {} as Asking<Name>
): Promise<
  OperationResponses[Name] extends { items: (infer Item)[] }
    ? Item[]
    : never
> {
  const all: unknown[] = []
  let after: string | undefined

  for (let page = 0; page < 20; page += 1) {
    const answer = (await api(operation, {
      ...(args as Record<string, unknown>),
      query: {
        ...(args as { query?: Record<string, unknown> }).query,
        after,
      },
    } as Asking<Name>)) as PageAnswer

    all.push(...answer.items)

    if (!answer.next_cursor) {
      break
    }

    after = answer.next_cursor
  }

  return all as never
}

function refused(error: unknown): ApiRefused | unknown {
  if (!(error instanceof MaviApiError)) {
    return error
  }

  const body = error.payload?.error

  return new ApiRefused(
    error.status,
    body?.code ?? "request_failed",
    body?.field ?? null,
    body?.message ?? `Mavi request failed with status ${error.status}`
  )
}
