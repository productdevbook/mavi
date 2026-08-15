/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Film, Loader2, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Curriculum, OnCourse, Video } from "../../server/types/mavicms"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

export const Route = createFileRoute("/dashboard/courses/$courseId")({
  component: CourseRoute,
})

/**
 * A course, module by module.
 *
 * The order is a number the API keeps, and two things cannot sit in one place:
 * moving something is saying where it goes, and being told no is better than
 * two lessons quietly numbered the same.
 */
function CourseRoute() {
  const { t } = useLingui()
  const { courseId } = Route.useParams()

  const [course, setCourse] = React.useState<Curriculum | null>(null)
  const [videos, setVideos] = React.useState<Video[]>([])
  const [moduleTitle, setModuleTitle] = React.useState("")
  const [lessonTitles, setLessonTitles] = React.useState<Record<string, string>>({})
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    api("GET /api/courses/{id}", { path: { id: courseId } })
      .then(setCourse)
      .catch((why: unknown) => {
        toast.error(said(why))
        setCourse(null)
      })
  }, [courseId])

  React.useEffect(load, [load])

  React.useEffect(() => {
    every("GET /api/videos")
      .then(setVideos)
      .catch((why: unknown) => {
        toast.error(said(why))
        setVideos((held) => held ?? [])
      })
  }, [])

  const open = async (state: string) => {
    try {
      await api("PATCH /api/courses/{id}", {
        path: { id: courseId },
        body: { state },
      })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const addModule = async () => {
    setBusy(true)

    try {
      await api("POST /api/courses/{id}/modules", {
        path: { id: courseId },
        body: { title: moduleTitle.trim() },
      })
      setModuleTitle("")
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  const addLesson = async (moduleId: string) => {
    const title = (lessonTitles[moduleId] ?? "").trim()

    if (!title) return

    try {
      await api("POST /api/modules/{id}/lessons", {
        path: { id: moduleId },
        body: { title },
      })
      setLessonTitles((held) => ({ ...held, [moduleId]: "" }))
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const plays = async (lessonId: string, videoId: string) => {
    try {
      await api("PATCH /api/lessons/{id}", {
        path: { id: lessonId },
        body: { video_id: videoId },
      })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const removeModule = async (id: string) => {
    try {
      await api("DELETE /api/modules/{id}", { path: { id } })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  const removeLesson = async (id: string) => {
    try {
      await api("DELETE /api/lessons/{id}", { path: { id } })
      load()
    } catch (why) {
      toast.error(said(why))
    }
  }

  if (!course) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="text-lg font-semibold">{course.course.title}</h1>
        <Badge variant={course.course.state === "open" ? "default" : "secondary"}>
          {course.course.state === "open" ? t`Open` : t`Being written`}
        </Badge>
        <Button
          variant="outline"
          size="sm"
          className="ml-auto"
          onClick={() =>
            void open(course.course.state === "open" ? "draft" : "open")
          }
        >
          {course.course.state === "open" ? t`Close it` : t`Open it`}
        </Button>
      </div>

      {course.modules.map((module) => (
        <section
          key={module.id}
          className="flex flex-col gap-3 rounded-xl border border-border p-4"
        >
          <div className="flex items-center gap-2">
            <input
              className="flex-1 bg-transparent text-sm font-medium outline-none"
              defaultValue={module.title}
              aria-label={t`What this part is called`}
              onBlur={(event) => {
                const wanted = event.target.value.trim()

                if (wanted && wanted !== module.title) {
                  void api("PATCH /api/modules/{id}", {
                    path: { id: module.id },
                    body: { title: wanted },
                  })
                    .then(load)
                    .catch((why: unknown) => toast.error(said(why)))
                }
              }}
            />
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

                  <Select
                    value={lesson.video_id ?? ""}
                    onValueChange={(value) =>
                      value && void plays(lesson.id, value)
                    }
                  >
                    <SelectTrigger size="sm" className="w-48">
                      <SelectValue placeholder={t`No video`} />
                    </SelectTrigger>
                    <SelectContent>
                      {videos.map((video) => (
                        <SelectItem key={video.id} value={video.id}>
                          {video.title}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>

                  {lesson.video_id && (
                    <Film className="size-4 text-muted-foreground" />
                  )}

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

      <OnIt courseId={courseId} />

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
function OnIt({ courseId }: { courseId: string }) {
  const { t } = useLingui()
  const [people, setPeople] = React.useState<OnCourse[] | null>(null)

  React.useEffect(() => {
    every("GET /api/courses/{id}/students", { path: { id: courseId } })
      .then(setPeople)
      .catch((why: unknown) => {
        toast.error(said(why))
        setPeople((held) => held ?? [])
      })
  }, [courseId])

  if (!people || people.length === 0) {
    return null
  }

  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-sm font-medium">{t`Who is on it`}</h2>

      <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
        {people.map((one) => (
          <div key={one.student_id} className="flex items-center gap-3 px-3 py-2">
            <span className="min-w-0 flex-1 truncate text-sm">
              {one.name || one.email}
            </span>
            <span className="text-xs text-muted-foreground">
              {new Date(one.enrolled_at).toLocaleDateString()}
            </span>
          </div>
        ))}
      </div>
    </section>
  )
}
