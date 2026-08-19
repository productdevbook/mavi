/**
 * Talking to the API, with the types the API itself wrote.
 *
 * The old client kept its own copy of every shape, and two copies of a shape
 * disagree the first time one changes — the wrong one always being the one
 * nobody is looking at. `@api` — generated from the API's own
 * description and a test refuses to pass while it is stale, so this file is
 * only the plumbing: one place that knows how a call is made, what an error
 * looks like, and what a page is.
 */

import type { Calls } from "@api"
import { operations } from "@api"

export interface Page<T> {
  items: T[]
  next?: string
}

export type AllCalls = Calls

/** Every call there is, as the API describes them. */
export type Call = keyof AllCalls

/** What a call takes and gives, for one call. */
export type Takes<K extends Call> = AllCalls[K]["takes"]
export type Gives<K extends Call> = AllCalls[K]["gives"]

/** Calls whose contract actually returns a cursor page. */
type PagedCall = {
  [K in Call]: Gives<K> extends Page<unknown> ? K : never
}[Call]

/**
 * What came back when it did not work.
 *
 * `key` is what the panel translates; `message` is English and is for a log or
 * for a screen that has no wording of its own yet. `named` holds whatever the
 * sentence names — a field, a count — so a translation can put them where its
 * own grammar wants them.
 */
export class Refused extends Error {
  readonly status: number
  readonly code: string
  readonly key: string | null
  readonly named: Record<string, string>

  constructor(
    status: number,
    code: string,
    key: string | null,
    named: Record<string, string>,
    message: string,
  ) {
    super(message)
    this.name = "Refused"
    this.status = status
    this.code = code
    this.key = key
    this.named = named
  }

  /** Whether this is somebody being told no rather than something breaking. */
  get expected(): boolean {
    return this.status < 500
  }
}

interface Asking<K extends Call> {
  /** What goes where a `{name}` is in the path. */
  path?: Record<string, string>
  query?: Record<string, string | number | boolean | undefined>
  body?: Takes<K> extends never ? undefined : Takes<K>
  signal?: AbortSignal
}

/**
 * Reads in flight, by what they are asking for.
 *
 * Two components mounting at once and asking the same question made two
 * requests; a screen with four panels on one listing made four. What is here
 * is not a cache — nothing is kept after it answers — it is the same request
 * being awaited twice.
 *
 * Anything that changes something clears it, because a read that started
 * before a write and is still in flight is an answer from before the write.
 */
const inFlight = new Map<string, Promise<unknown>>()

/**
 * Makes one call.
 *
 * Every call is a generated operation name. A screen cannot invent a path or
 * method here: the contract is the only place that decides where an operation
 * lives.
 */
export async function api<K extends Call>(
  call: K,
  asking: Asking<K> = {} as Asking<K>,
): Promise<Gives<K>> {
  const operation = operations[call as keyof typeof operations]
  const method = operation.method.toUpperCase()
  const template = operation.path
  const path = fill(template, asking.path ?? {})
  const url = path + search(asking.query)

  if (method === "GET" && !asking.signal) {
    const held = inFlight.get(url)

    if (held) {
      return held as Promise<Gives<K>>
    }

    const asked = ask<K>(method, url, asking).finally(() => {
      inFlight.delete(url)
    })

    inFlight.set(url, asked)

    return asked
  }

  if (method !== "GET") {
    inFlight.clear()
  }

  return ask<K>(method, url, asking)
}

async function ask<K extends Call>(
  method: string,
  url: string,
  asking: Asking<K>,
): Promise<Gives<K>> {
  const response = await fetch(url, {
    method,
    // Sent so the site's own address is what answers: every call here is to
    // the site the panel is open on, never to another one.
    credentials: "same-origin",
    headers: asking.body === undefined
      ? {}
      : { "content-type": "application/json" },
    body: asking.body === undefined ? undefined : JSON.stringify(asking.body),
    signal: asking.signal,
  })

  if (response.status === 204) {
    return undefined as Gives<K>
  }

  const body = await response.json().catch(() => null)

  if (!response.ok) {
    throw refusal(response.status, body)
  }

  return body as Gives<K>
}

/** Everything on a listing, as one array — for the screens that show a list. */
export async function every<K extends Call>(
  call: K extends PagedCall ? K : never,
  asking: Asking<K> = {} as Asking<K>,
): Promise<Gives<K> extends Page<infer T> ? T[] : never> {
  const all: unknown[] = []
  let after: string | undefined

  // Bounded on purpose: a screen that walks forty pages is a screen that should
  // be asking for one, and a loop with no end is how a panel hangs on a site
  // with ten thousand posts.
  for (let page = 0; page < 20; page += 1) {
    const answer = (await api(call, {
      ...asking,
      query: { ...asking.query, after },
    })) as Page<unknown>

    all.push(...answer.items)

    if (!answer.next) {
      break
    }

    after = answer.next
  }

  return all as never
}

function fill(template: string, values: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (_whole, name: string) => {
    const value = values[name]

    if (value === undefined) {
      throw new Error(`${template} wants ${name} and nothing was given`)
    }

    return encodeURIComponent(value)
  })
}

function search(
  query: Record<string, string | number | boolean | undefined> | undefined,
): string {
  if (!query) {
    return ""
  }

  const asked = new URLSearchParams()

  for (const [name, value] of Object.entries(query)) {
    if (value !== undefined) {
      asked.set(name, String(value))
    }
  }

  const written = asked.toString()

  return written ? `?${written}` : ""
}

function refusal(status: number, body: unknown): Refused {
  const said =
    body && typeof body === "object" && "error" in body
      ? (body as { error: Record<string, unknown> }).error
      : null

  return new Refused(
    status,
    typeof said?.code === "string" ? said.code : "internal",
    typeof said?.key === "string" ? said.key : null,
    said?.named && typeof said.named === "object"
      ? (said.named as Record<string, string>)
      : {},
    typeof said?.message === "string" ? said.message : `HTTP ${status}`,
  )
}
