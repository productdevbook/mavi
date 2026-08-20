import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import {
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  GraduationCap,
  Loader2,
  Play,
} from "lucide-react"

import * as learn from "@/learn/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Player } from "@/learn/player"

/**
 * Three screens and a path switch.
 *
 * A route tree would be a generated file to keep in sync for the sake of three
 * addresses.
 */
type Where =
  | { at: "courses" }
  | { at: "course"; id: string }
  | { at: "lesson"; id: string }

const ROOT = "/learn"

function read(): Where {
  const path = window.location.pathname
    .replace(ROOT, "")
    .replace(/^\/|\/$/g, "")

  const parts = path.split("/").filter(Boolean)

  if (parts[0] === "courses" && parts[1]) return { at: "course", id: parts[1] }
  if (parts[0] === "lessons" && parts[1]) return { at: "lesson", id: parts[1] }

  return { at: "courses" }
}

function go(to: string) {
  window.history.pushState({}, "", `${ROOT}${to}`)
  window.dispatchEvent(new PopStateEvent("popstate"))
}

export function App() {
  const { t } = useLingui()
  const [where, setWhere] = React.useState<Where>(read)
  const [who, setWho] = React.useState<learn.Learner | null>(null)
  const [loading, setLoading] = React.useState(true)

  React.useEffect(() => {
    const onMove = () => setWhere(read())

    window.addEventListener("popstate", onMove)

    return () => window.removeEventListener("popstate", onMove)
  }, [])

  const load = React.useCallback(() => {
    learn
      .me()
      .then(setWho)
      .catch(() => setWho(null))
      .finally(() => setLoading(false))
  }, [])

  React.useEffect(load, [load])

  if (loading) {
    return (
      <div className="flex min-h-svh items-center justify-center">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  if (!who) {
    return <SignIn onDone={load} />
  }

  return (
    <div className="min-h-svh bg-background">
      <header className="flex items-center gap-3 border-b border-border px-4 py-3">
        <button
          className="text-sm font-medium"
          onClick={() => go("/")}
        >{t`My courses`}</button>

        <span className="ml-auto truncate text-sm text-muted-foreground">
          {who.name || who.email}
        </span>

        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            void learn.signOut().finally(() => setWho(null))
          }}
        >
          {t`Sign out`}
        </Button>
      </header>

      <main
        className={`mx-auto w-full px-4 py-6 sm:px-6 sm:py-10 ${
          where.at === "lesson" ? "max-w-6xl" : "max-w-3xl"
        }`}
      >
        {where.at === "courses" && <Courses />}
        {where.at === "course" && <Course id={where.id} />}
        {where.at === "lesson" && <Lesson key={where.id} id={where.id} />}
      </main>
    </div>
  )
}

function SignIn({ onDone }: { onDone: () => void }) {
  const { t } = useLingui()
  const [email, setEmail] = React.useState("")
  const [password, setPassword] = React.useState("")
  const [busy, setBusy] = React.useState(false)
  const [refused, setRefused] = React.useState("")

  const send = async (event: React.FormEvent) => {
    event.preventDefault()
    setBusy(true)
    setRefused("")

    try {
      await learn.signIn(email.trim(), password)
      onDone()
    } catch (why) {
      setRefused(
        why instanceof learn.LearnError ? why.message : t`Something failed`,
      )
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="flex min-h-svh items-center justify-center px-4">
      <form
        onSubmit={(event) => void send(event)}
        className="flex w-full max-w-sm flex-col gap-4 rounded-xl border border-border px-6 py-6"
      >
        <div>
          <h1 className="text-lg font-semibold">{t`Sign in`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`To watch the courses you have been given.`}
          </p>
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="email">{t`Email`}</Label>
          <Input
            id="email"
            type="email"
            autoComplete="email"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
          />
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="password">{t`Password`}</Label>
          <Input
            id="password"
            type="password"
            autoComplete="current-password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
          />
        </div>

        {refused && (
          <p className="rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {refused}
          </p>
        )}

        <Button type="submit" disabled={busy || !email || !password}>
          {busy ? <Loader2 className="animate-spin" /> : null}
          {t`Sign in`}
        </Button>

        <p className="text-xs text-muted-foreground">
          {t`Whoever put you on the course gave you a password. If it is lost, ask them for another.`}
        </p>
      </form>
    </div>
  )
}

function Courses() {
  const { t } = useLingui()
  const [courses, setCourses] = React.useState<learn.Course[] | null>(null)

  React.useEffect(() => {
    learn
      .mine()
      .then((page) => setCourses(page.items))
      .catch(() => setCourses([]))
  }, [])

  if (courses === null) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  if (courses.length === 0) {
    return (
      <div className="rounded-xl border border-dashed border-border py-16 text-center">
        <GraduationCap className="mx-auto mb-3 size-8 text-muted-foreground" />
        <p className="text-sm text-muted-foreground">
          {t`Nothing yet. When somebody puts you on a course it appears here.`}
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-3">
      <h1 className="text-lg font-semibold">{t`My courses`}</h1>

      {courses.map((course) => (
        <button
          key={course.course_id}
          type="button"
          className="flex flex-col gap-1 rounded-xl border border-border px-4 py-3 text-left hover:bg-muted/50"
          onClick={() => go(`/courses/${course.course_id}`)}
        >
          <span className="text-sm font-medium">{course.title}</span>
          {course.about && (
            <span className="text-xs text-muted-foreground">
              {course.about}
            </span>
          )}
        </button>
      ))}
    </div>
  )
}

function Course({ id }: { id: string }) {
  const { t } = useLingui()
  const [whole, setWhole] = React.useState<learn.Curriculum | null>(null)

  React.useEffect(() => {
    learn
      .course(id)
      .then(setWhole)
      .catch(() => setWhole(null))
  }, [id])

  if (!whole) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const lessons = whole.modules.flatMap((module) => module.lessons)
  const done = lessons.filter((lesson) => lesson.completed_at !== null).length

  return (
    <div className="flex flex-col gap-5">
      <div>
        <h1 className="text-lg font-semibold">{whole.course.title}</h1>
        <p className="text-sm text-muted-foreground">
          {t`${done} of ${lessons.length} finished`}
        </p>
      </div>

      {whole.modules.map((module) => (
        <section key={module.id} className="flex flex-col gap-2">
          <h2 className="text-sm font-medium">{module.title}</h2>

          <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
            {module.lessons.map((lesson) => (
              <button
                key={lesson.id}
                type="button"
                className="flex items-center gap-3 px-4 py-3 text-left hover:bg-muted/50"
                onClick={() => go(`/lessons/${lesson.id}`)}
              >
                {lesson.completed_at ? (
                  <CheckCircle2 className="size-4 shrink-0 text-emerald-600" />
                ) : (
                  <Play className="size-4 shrink-0 text-muted-foreground" />
                )}
                <span className="min-w-0 flex-1 truncate text-sm">
                  {lesson.title}
                </span>
              </button>
            ))}
          </div>
        </section>
      ))}
    </div>
  )
}

function Lesson({ id }: { id: string }) {
  const { t } = useLingui()
  const [watching, setWatching] = React.useState<learn.Watching | null>(null)
  const [refused, setRefused] = React.useState("")

  const load = React.useCallback(() => {
    learn
      .lesson(id)
      .then(setWatching)
      .catch((why: unknown) =>
        setRefused(
          why instanceof learn.LearnError
            ? why.message
            : "Something went wrong.",
        ),
      )
  }, [id])

  React.useEffect(load, [load])

  if (refused) {
    return <p className="text-sm text-muted-foreground">{refused}</p>
  }

  if (!watching) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const finish = async () => {
    await learn.finished(id).catch(() => {})
    load()
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          className="text-sm text-muted-foreground hover:underline"
          onClick={() => go(`/courses/${watching.course_id}`)}
        >
          {watching.course}
        </button>
        <span className="text-xs text-muted-foreground">
          {t`${watching.position} of ${watching.total}`}
        </span>
      </div>

      <h1 className="text-lg font-semibold">{watching.lesson.title}</h1>

      {watching.lesson.media_file_id && (
        <Player lessonId={watching.lesson.id} />
      )}

      {watching.lesson.body && (
        <article className="mavi-prose whitespace-pre-wrap text-sm">
          {watching.lesson.body}
        </article>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="outline"
          disabled={!watching.previous}
          onClick={() =>
            watching.previous && go(`/lessons/${watching.previous}`)
          }
        >
          <ChevronLeft /> {t`Back`}
        </Button>

        <Button
          variant={watching.completed_at ? "outline" : "default"}
          onClick={() => void finish()}
          disabled={watching.completed_at !== null}
        >
          <CheckCircle2 />
          {watching.completed_at ? t`Finished` : t`Mark as finished`}
        </Button>

        <Button
          className="ml-auto"
          disabled={!watching.next}
          onClick={() => watching.next && go(`/lessons/${watching.next}`)}
        >
          {t`Next`} <ChevronRight />
        </Button>
      </div>
    </div>
  )
}
