/**
 * What a student's browser is allowed to ask for.
 *
 * Deliberately small, and deliberately not the panel's own client: that one
 * names every endpoint a site has, and importing it here would put the shape
 * of the whole administrative API into a bundle served to people who bought a
 * course. Nothing is exposed by a type — but nothing here needs one either,
 * and the smaller surface is the point.
 */

export class LearnError extends Error {
  status: number

  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function ask<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api${path}`, {
    credentials: "same-origin",
    headers: init?.body ? { "content-type": "application/json" } : {},
    ...init,
  })

  if (!response.ok) {
    const body = await response.json().catch(() => null)

    throw new LearnError(
      response.status,
      String(body?.error?.message ?? response.statusText),
    )
  }

  if (response.status === 204) {
    return undefined as T
  }

  return response.json() as Promise<T>
}

/** One course somebody is on. */
export interface Course {
  id: string
  slug: string
  title: string
  summary: string | null
  state: string
}

export interface Lesson {
  id: string
  title: string
  position: number
  video_id: string | null
  done: boolean
}

export interface Module {
  id: string
  title: string
  position: number
  lessons: Lesson[]
}

export interface Curriculum {
  course: Course
  modules: Module[]
}

/** One lesson, as somebody on the course reads it. */
export interface Watching {
  id: string
  title: string
  body: string
  course_id: string
  course: string
  position: number
  total: number
  previous: string | null
  next: string | null
  video_id: string | null
  done: boolean
}

export interface Learner {
  id: string
  email: string
  name: string
}

export const me = () => ask<Learner>("/learn/me")

export const signIn = (email: string, password: string) =>
  ask<void>("/learn/session", {
    method: "POST",
    body: JSON.stringify({ email, password }),
  })

export const signOut = () => ask<void>("/learn/session", { method: "DELETE" })

export const mine = () =>
  ask<{ items: Course[]; next: string | null }>("/learn/courses")

export const course = (id: string) => ask<Curriculum>(`/learn/courses/${id}`)

export const lesson = (id: string) => ask<Watching>(`/learn/lessons/${id}`)

export const finished = (id: string) =>
  ask<void>(`/learn/lessons/${id}/done`, { method: "POST" })

/** Where a lesson's video is played from. Refused to anybody not on it. */
export const watching = (videoId: string) => `/api/learn/videos/${videoId}`
