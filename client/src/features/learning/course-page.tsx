import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { Course, Student } from "@api"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"

export function CoursePage({ courseId }: { courseId: string }) {
  const { t } = useLingui()

  const [course, setCourse] = React.useState<Course | null>(null)
  const [moduleTitle, setModuleTitle] = React.useState("")
  const [lessonTitles, setLessonTitles] = React.useState<
    Record<string, string>
  >({})
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    api("courses.read", { path: { id: courseId } })
      .then(setCourse)
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setCourse(null)
      })
  }, [courseId])

  React.useEffect(load, [load])

  const open = async (state: "draft" | "open" | "closed") => {
    try {
      await api("courses.update", {
        path: { id: courseId },
        body: { state },
      })
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  const addModule = async () => {
    setBusy(true)

    try {
      await api("courses.modules.create", {
        path: { id: courseId },
        body: { title: moduleTitle.trim() },
      })
      setModuleTitle("")
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    } finally {
      setBusy(false)
    }
  }

  const addLesson = async (moduleId: string) => {
    const title = (lessonTitles[moduleId] ?? "").trim()

    if (!title) return

    try {
      await api("courses.lessons.create", {
        path: { id: moduleId },
        body: { title, body: "" },
      })
      setLessonTitles((held) => ({ ...held, [moduleId]: "" }))
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  const removeModule = async (id: string) => {
    try {
      await api("courses.modules.delete", { path: { id } })
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  const removeLesson = async (id: string) => {
    try {
      await api("courses.lessons.delete", { path: { id } })
      load()
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  if (!course) {
    return <DashboardLoading />
  }

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <DashboardPageHeader
        title={course.title}
        description={t`Build modules and lessons for this course.`}
        actions={
          <div className="flex items-center gap-2">
            <Badge variant={course.state === "open" ? "default" : "secondary"}>
              {course.state === "open" ? t`Open` : t`Being written`}
            </Badge>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                void open(course.state === "open" ? "draft" : "open")
              }
            >
              {course.state === "open" ? t`Close it` : t`Open it`}
            </Button>
          </div>
        }
      />

      {course.modules.map((module) => (
        <section
          key={module.id}
          className="flex flex-col gap-3 rounded-xl border border-border p-4"
        >
          <div className="flex items-center gap-2">
            <span className="flex-1 text-sm font-medium">{module.title}</span>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t`Remove`}
              onClick={() => void removeModule(module.id)}
            >
              <Trash2 />
            </Button>
          </div>

          <div className="flex flex-col divide-y divide-border rounded-lg border border-border">
            {module.lessons.length === 0 ? (
              <p className="px-3 py-2 text-xs text-muted-foreground">
                {t`No lessons in it yet.`}
              </p>
            ) : (
              module.lessons.map((lesson) => (
                <div
                  key={lesson.id}
                  className="flex flex-wrap items-center gap-2 px-3 py-2"
                >
                  <span className="min-w-0 flex-1 truncate text-sm">
                    {lesson.title}
                  </span>

                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t`Remove`}
                    onClick={() => void removeLesson(lesson.id)}
                  >
                    <Trash2 />
                  </Button>
                </div>
              ))
            )}
          </div>

          <form
            className="flex gap-2"
            onSubmit={(event) => {
              event.preventDefault()
              void addLesson(module.id)
            }}
          >
            <Input
              value={lessonTitles[module.id] ?? ""}
              placeholder={t`A lesson`}
              onChange={(event) =>
                setLessonTitles((held) => ({
                  ...held,
                  [module.id]: event.target.value,
                }))
              }
            />
            <Button type="submit" variant="outline">
              <Plus /> {t`Add`}
            </Button>
          </form>
        </section>
      ))}

      <OnIt />

      <form
        className="flex max-w-md gap-2"
        onSubmit={(event) => {
          event.preventDefault()
          void addModule()
        }}
      >
        <Input
          value={moduleTitle}
          placeholder={t`A part of the course`}
          onChange={(event) => setModuleTitle(event.target.value)}
        />
        <Button type="submit" disabled={!moduleTitle.trim() || busy}>
          {busy ? <Loader2 className="animate-spin" /> : <Plus />}
          {t`Add a part`}
        </Button>
      </form>
    </div>
  )
}

/** Who is on this course, which is the other half of writing one. */
function OnIt() {
  const { t } = useLingui()
  const [people, setPeople] = React.useState<Student[] | null>(null)

  React.useEffect(() => {
    every("courses.students.list", { query: {} })
      .then(setPeople)
      .catch((why: unknown) => {
        toast.error(apiMessage(why))
        setPeople((held) => held ?? [])
      })
  }, [])

  if (!people || people.length === 0) {
    return null
  }

  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-sm font-medium">{t`Students`}</h2>

      <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
        {people.map((one) => (
          <div key={one.id} className="flex items-center gap-3 px-3 py-2">
            <span className="min-w-0 flex-1 truncate text-sm">
              {one.name || one.email}
            </span>
            <span className="text-xs text-muted-foreground">
              {new Date(one.created_at).toLocaleDateString()}
            </span>
          </div>
        ))}
      </div>
    </section>
  )
}
