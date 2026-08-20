import {
  MaviApiError,
  MaviClient,
} from "@api"
import type {
  LearningCourse,
  LearningCourseDetail,
  LearningCoursePage,
  LearningLesson,
  OperationArguments,
  OperationName,
  OperationResponses,
  Student,
  StudentSessionCreated,
} from "@api"

export type Learner = Student
export type Course = LearningCourse
export type Curriculum = LearningCourseDetail
export type Watching = LearningLesson

/** The student bundle speaks only the canonical student contract. */
const SESSION_KEY = "mavi:student-session"

const STUDENT_OPERATIONS = [
  "courses.students.session.create",
  "courses.students.session.revoke",
  "learning.courses.list",
  "learning.course.read",
  "learning.lesson.read",
  "learning.lesson.media.read",
  "learning.lesson.done",
] as const satisfies readonly OperationName[]

type StudentOperation = (typeof STUDENT_OPERATIONS)[number]

export class LearnError extends Error {
  readonly status: number
  readonly code: string

  constructor(status: number, message: string, code = "request_failed") {
    super(message)
    this.name = "LearnError"
    this.status = status
    this.code = code
  }
}

function readSession(): StudentSessionCreated | null {
  const encoded = window.sessionStorage.getItem(SESSION_KEY)
  if (!encoded) return null

  try {
    const session = JSON.parse(encoded) as StudentSessionCreated
    if (
      typeof session.token !== "string" ||
      typeof session.student?.id !== "string" ||
      Date.parse(session.expires_at) <= Date.now()
    ) {
      forgetSession()
      return null
    }
    return session
  } catch {
    forgetSession()
    return null
  }
}

function rememberSession(session: StudentSessionCreated): void {
  window.sessionStorage.setItem(SESSION_KEY, JSON.stringify(session))
}

function forgetSession(): void {
  window.sessionStorage.removeItem(SESSION_KEY)
}

function client(): MaviClient {
  return new MaviClient({
    baseUrl: window.location.origin,
    token: readSession()?.token,
  })
}

async function call<Name extends StudentOperation>(
  operation: Name,
  args: OperationArguments[Name],
): Promise<OperationResponses[Name]> {
  try {
    return await client().call(operation, args)
  } catch (error) {
    if (!(error instanceof MaviApiError)) throw error
    const body = error.payload?.error
    throw new LearnError(
      error.status,
      body?.message ?? `Mavi request failed with status ${error.status}`,
      body?.code,
    )
  }
}

/** The signed-in student is the identity returned by the canonical session. */
export const me = async (): Promise<Student> => {
  const session = readSession()
  if (!session) {
    throw new LearnError(401, "You are not signed in.", "unauthenticated")
  }
  return session.student
}

export const signIn = async (
  email: string,
  password: string,
): Promise<Student> => {
  const session = await call("courses.students.session.create", {
    body: { email, password },
  })
  rememberSession(session)
  return session.student
}

export const signOut = async (): Promise<void> => {
  if (!readSession()) return
  try {
    await call("courses.students.session.revoke", {})
  } finally {
    forgetSession()
  }
}

export const mine = (): Promise<LearningCoursePage> =>
  call("learning.courses.list", { query: {} })

export const course = (id: string): Promise<LearningCourseDetail> =>
  call("learning.course.read", { path: { id } })

export const lesson = (id: string): Promise<LearningLesson> =>
  call("learning.lesson.read", { path: { id } })

export const finished = (id: string) =>
  call("learning.lesson.done", { path: { id } })

/** Private lesson media is authorized by the student session on every read. */
export const watching = (lessonId: string): string =>
  `/student/v1/learning/lessons/${encodeURIComponent(lessonId)}/media`
