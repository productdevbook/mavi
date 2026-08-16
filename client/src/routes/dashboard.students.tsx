/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Ban, Loader2, Plus, Trash2, UserPlus } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Course, Enrolment, Student } from "@api"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

export const Route = createFileRoute("/dashboard/students")({
  component: StudentsRoute,
})

/// The lengths somebody actually sells. Not a rule — "no end" is the one at
/// the bottom, and any of them can be given again later.
const LENGTHS = [30, 90, 180, 365]

/**
 * Who may watch what, and until when.
 *
 * Nobody types anybody's password: putting somebody on a course makes their
 * account and hands back a password once, and it is not stored anywhere it can
 * be read again.
 */
function StudentsRoute() {
  const { t, i18n } = useLingui()
  const [students, setStudents] = React.useState<Student[] | null>(null)
  const [courses, setCourses] = React.useState<Course[]>([])
  const [enrolments, setEnrolments] = React.useState<
    Record<string, Enrolment[]>
  >({})
  const [giving, setGiving] = React.useState<Student | "somebody new" | null>(
    null,
  )
  const [password, setPassword] = React.useState<{
    who: string
    secret: string
  } | null>(null)

  const load = React.useCallback(() => {
    every("GET /api/students")
      .then(async (found) => {
        setStudents(found)

        const theirs = await Promise.all(
          found.map((student) =>
            api("GET /api/students/{id}/enrolments", {
              path: { id: student.id },
            })
              .then((rows) => [student.id, rows] as const)
              .catch(() => [student.id, [] as Enrolment[]] as const),
          ),
        )

        setEnrolments(Object.fromEntries(theirs))
      })
      .catch((why: unknown) => {
        toast.error(said(why))
        setStudents((held) => held ?? [])
      })
  }, [])

  React.useEffect(load, [load])

  React.useEffect(() => {
    every("GET /api/courses")
      .then(setCourses)
      .catch((why: unknown) => {
        toast.error(said(why))
        setCourses((held) => held ?? [])
      })
  }, [])

  const when = (iso: string) =>
    new Date(iso).toLocaleDateString(i18n.locale, { dateStyle: "medium" })

  const suspend = async (student: Student) => {
    try {
      await api("PATCH /api/students/{id}", {
        path: { id: student.id },
        body: { suspended: student.state !== "suspended" },
      })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const longer = async (enrolment: Enrolment, days: number) => {
    try {
      await api("PATCH /api/enrolments/{id}", {
        path: { id: enrolment.id },
        body: days === 0 ? { forever: true } : { days },
      })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const revoke = async (enrolment: Enrolment) => {
    try {
      await api("DELETE /api/enrolments/{id}", { path: { id: enrolment.id } })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const states: Record<string, string> = {
    waiting: t`Not started yet`,
    open: t`Access running`,
    ended: t`Access ended`,
  }

  return (
    <>
      <div className="mb-6 flex flex-col items-start gap-4 sm:flex-row sm:justify-between">
        <div>
          <h1 className="text-lg font-semibold">{t`Students`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`Who may watch what, and until when. Putting somebody on a course makes their account and shows their password once.`}
          </p>
        </div>
        <Button className="shrink-0" onClick={() => setGiving("somebody new")}>
          <UserPlus /> {t`Add somebody`}
        </Button>
      </div>

      {students === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : students.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border py-16 text-center text-sm text-muted-foreground">
          {t`Nobody yet`}
        </p>
      ) : (
        <div className="flex max-w-4xl flex-col gap-3">
          {students.map((student) => (
            <div
              key={student.id}
              className="rounded-xl border border-border px-4 py-3"
            >
              <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
                <div className="min-w-0 basis-full sm:flex-1 sm:basis-0">
                  <p className="truncate text-sm font-medium">
                    {student.name || student.email}
                    {student.state === "suspended" && (
                      <Badge variant="secondary" className="ml-2">
                        {t`Stopped`}
                      </Badge>
                    )}
                  </p>
                  <p className="truncate text-xs text-muted-foreground">
                    {student.email}
                    {student.last_seen_at
                      ? ` · ${t`last here ${when(student.last_seen_at)}`}`
                      : ` · ${t`never signed in`}`}
                  </p>
                </div>

                <Button
                  variant="outline"
                  size="sm"
                  className="ml-auto"
                  onClick={() => setGiving(student)}
                >
                  <Plus /> {t`Give access`}
                </Button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={
                    student.state === "suspended" ? t`Let back in` : t`Stop`
                  }
                  onClick={() => void suspend(student)}
                >
                  <Ban />
                </Button>
              </div>

              {(enrolments[student.id] ?? []).length > 0 && (
                <div className="mt-3 flex flex-col divide-y divide-border rounded-lg border border-border">
                  {(enrolments[student.id] ?? []).map((one) => (
                    <div
                      key={one.id}
                      className="flex flex-wrap items-center gap-x-3 gap-y-2 px-3 py-2"
                    >
                      <div className="min-w-0 basis-full sm:flex-1 sm:basis-0">
                        <p className="truncate text-sm">{one.course}</p>
                        <p className="truncate text-xs text-muted-foreground">
                          {one.ends_at
                            ? t`until ${when(one.ends_at)}`
                            : t`no end date`}
                        </p>
                      </div>
                      <Badge
                        variant={one.state === "open" ? "default" : "secondary"}
                        className="ml-auto"
                      >
                        {states[one.state] ?? one.state}
                      </Badge>
                      <Select
                        value=""
                        onValueChange={(value) =>
                          void longer(one, Number(value ?? 0))
                        }
                      >
                        <SelectTrigger className="w-40" size="sm">
                          <SelectValue placeholder={t`Give longer`} />
                        </SelectTrigger>
                        <SelectContent>
                          {LENGTHS.map((days) => (
                            <SelectItem key={days} value={String(days)}>
                              {t`${days} days more`}
                            </SelectItem>
                          ))}
                          <SelectItem value="0">{t`No end date`}</SelectItem>
                        </SelectContent>
                      </Select>
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label={t`Take it back`}
                        onClick={() => void revoke(one)}
                      >
                        <Trash2 />
                      </Button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Keyed on who, so choosing a second person gets a fresh form rather
          than an effect resetting four fields after the fact. */}
      <GiveAccess
        key={typeof giving === "string" ? giving : (giving?.id ?? "nobody")}
        student={giving}
        courses={courses}
        onClose={() => setGiving(null)}
        onDone={(who, secret) => {
          setGiving(null)

          if (secret) {
            setPassword({ who, secret })
          }

          load()
        }}
      />

      <Dialog open={password !== null} onOpenChange={() => setPassword(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t`A password for ${password?.who ?? ""}`}</DialogTitle>
            <DialogDescription>
              {t`Shown once and kept nowhere it can be read again. Send it however you normally reach them; if it is lost, give them access again and a new one appears.`}
            </DialogDescription>
          </DialogHeader>
          <code className="block overflow-x-auto rounded-md border border-border px-3 py-2 font-mono text-xs">
            {password?.secret}
          </code>
        </DialogContent>
      </Dialog>
    </>
  )
}

/**
 * Putting somebody on a course.
 *
 * The same call whether they are new or not: an address the site already
 * teaches keeps its account, and one it does not gets one — which is why the
 * password only comes back when there was nobody there before.
 */
function GiveAccess({
  student,
  courses,
  onClose,
  onDone,
}: {
  student: Student | "somebody new" | null
  courses: Course[]
  onClose: () => void
  onDone: (who: string, secret: string | null) => void
}) {
  const { t } = useLingui()
  const known = typeof student === "object" && student !== null

  const [email, setEmail] = React.useState(known ? student.email : "")
  const [name, setName] = React.useState(known ? student.name : "")
  const [course, setCourse] = React.useState("")
  const [days, setDays] = React.useState("0")
  const [busy, setBusy] = React.useState(false)

  const give = async () => {
    setBusy(true)

    try {
      const enrolled = await api("POST /api/courses/{id}/students", {
        path: { id: course },
        body: {
          email,
          name: name.trim() || email,
          days: days === "0" ? null : Number(days),
        },
      })

      onDone(email, known ? null : enrolled.token)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog open={student !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t`Give access`}</DialogTitle>
          <DialogDescription>
            {t`Which course, and for how long. Access with no end date runs until somebody takes it back.`}
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          <div className="flex flex-col gap-2">
            <Label htmlFor="student-email">{t`Email`}</Label>
            <Input
              id="student-email"
              type="email"
              value={email}
              disabled={known}
              onChange={(event) => setEmail(event.target.value)}
            />
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="student-name">{t`Name`}</Label>
            <Input
              id="student-name"
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="student-course">{t`Course`}</Label>
            <Select
              value={course}
              onValueChange={(value) => setCourse(value ?? "")}
            >
              <SelectTrigger id="student-course">
                <SelectValue placeholder={t`Which course`} />
              </SelectTrigger>
              <SelectContent>
                {courses.map((one) => (
                  <SelectItem key={one.id} value={one.id}>
                    {one.title}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="student-days">{t`For how long`}</Label>
            <Select
              value={days}
              onValueChange={(value) => setDays(value ?? "0")}
            >
              <SelectTrigger id="student-days">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="0">{t`No end date`}</SelectItem>
                {LENGTHS.map((length) => (
                  <SelectItem key={length} value={String(length)}>
                    {t`${length} days`}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>{t`Cancel`}</Button>
          <Button
            disabled={busy || !email.trim() || !course}
            onClick={() => void give()}
          >
            {busy ? <Loader2 className="animate-spin" /> : null}
            {t`Give access`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
