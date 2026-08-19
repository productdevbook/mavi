import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plus, UserPlus } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Course, Student } from "@legacy-api"
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
import {
  DashboardEmpty,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

/**
 * Who may watch what, and until when.
 *
 * Nobody types anybody's password: putting somebody on a course makes their
 * account and hands back a password once, and it is not stored anywhere it can
 * be read again.
 */
export function StudentsPage() {
  const { t, i18n } = useLingui()
  const [students, setStudents] = React.useState<Student[] | null>(null)
  const [courses, setCourses] = React.useState<Course[]>([])
  const [giving, setGiving] = React.useState<Student | "somebody new" | null>(
    null
  )

  const load = React.useCallback(() => {
    every("students.list")
      .then(setStudents)
      .catch((why: unknown) => {
        toast.error(said(why))
        setStudents((held) => held ?? [])
      })
  }, [])

  React.useEffect(load, [load])

  React.useEffect(() => {
    every("courses.list")
      .then(setCourses)
      .catch((why: unknown) => {
        toast.error(said(why))
        setCourses((held) => held ?? [])
      })
  }, [])

  const when = (iso: string) =>
    new Date(iso).toLocaleDateString(i18n.locale, { dateStyle: "medium" })

  return (
    <div className="flex flex-col gap-5">
      <DashboardPageHeader
        title={t`Students`}
        description={t`Who may watch what, and until when.`}
        actions={
          <Button
            className="shrink-0"
            onClick={() => setGiving("somebody new")}
          >
            <UserPlus /> {t`Add somebody`}
          </Button>
        }
      />

      {students === null ? (
        <DashboardLoading />
      ) : students.length === 0 ? (
        <DashboardEmpty
          icon={UserPlus}
          title={t`Nobody yet`}
          description={t`Add a student to enroll them in one of your courses.`}
          action={
            <Button onClick={() => setGiving("somebody new")}>
              <UserPlus /> {t`Add somebody`}
            </Button>
          }
        />
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
                  </p>
                  <p className="truncate text-xs text-muted-foreground">
                    {student.email} · {when(student.created_at)}
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
              </div>
            </div>
          ))}
        </div>
      )}

      <GiveAccess
        key={typeof giving === "string" ? giving : (giving?.id ?? "nobody")}
        student={giving}
        courses={courses}
        onClose={() => setGiving(null)}
        onDone={() => {
          setGiving(null)
          load()
        }}
      />
    </div>
  )
}

function GiveAccess({
  student,
  courses,
  onClose,
  onDone,
}: {
  student: Student | "somebody new" | null
  courses: Course[]
  onClose: () => void
  onDone: () => void
}) {
  const { t } = useLingui()
  const known = typeof student === "object" && student !== null

  const [email, setEmail] = React.useState(known ? student.email : "")
  const [name, setName] = React.useState(known ? student.name : "")
  const [course, setCourse] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  const give = async () => {
    setBusy(true)

    try {
      let studentId = known ? student.id : ""
      if (!known) {
        const created = await api("students.ask", {
          body: {
            email,
            name: name.trim() || email,
          },
        })
        studentId = created.id
      }

      await api("enrolments.add", {
        path: { id: course },
        body: {
          student: studentId,
        },
      })

      onDone()
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
            {t`Which course to enroll the student in.`}
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
              disabled={known}
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
