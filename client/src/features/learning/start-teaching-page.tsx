import * as React from "react"
import { useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2 } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { DashboardPageHeader } from "@/components/dashboard/dashboard-page"

/** An address out of a title: lower-case, dashes for gaps. */
function slugged(title: string): string {
  return title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
}

/**
 * Setting a site up to teach.
 *
 * A course is its own thing here rather than a kind of content: it has modules
 * and lessons, people are put on it for as long as somebody says, and what
 * they have finished is kept. This makes the first one; after that, the course
 * itself is what the sidebar points at.
 */
export function StartTeachingPage() {
  const { t } = useLingui()
  const navigate = useNavigate()

  const [title, setTitle] = React.useState("")
  const [slug, setSlug] = React.useState("")
  const [summary, setSummary] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  const make = async () => {
    setBusy(true)

    try {
      const course = await api("POST /api/courses", {
        body: {
          slug: slug.trim() || slugged(title),
          title: title.trim(),
          about: summary.trim() || null,
        },
      })

      void navigate({
        to: "/dashboard/courses/$courseId",
        params: { courseId: course.id },
      })
    } catch (why) {
      toast.error(said(why))
      setBusy(false)
    }
  }

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <DashboardPageHeader
        title={t`Start teaching`}
        description={t`A course holds modules and lessons, and people are put on it for as long as you say. Nothing is open to anybody until you say it is.`}
      />

      <div className="flex flex-col gap-1.5">
        <Label htmlFor="course-title">{t`What it is called`}</Label>
        <Input
          id="course-title"
          value={title}
          onChange={(event) => {
            setTitle(event.target.value)
            setSlug(slugged(event.target.value))
          }}
          placeholder={t`Photography, from the beginning`}
        />
        <p className="font-mono text-xs text-muted-foreground">
          /{slug || slugged(title)}
        </p>
      </div>

      <div className="flex flex-col gap-1.5">
        <Label htmlFor="course-summary">{t`What it is about`}</Label>
        <Textarea
          id="course-summary"
          value={summary}
          onChange={(event) => setSummary(event.target.value)}
          rows={3}
        />
      </div>

      <Button
        className="self-start"
        disabled={!title.trim() || busy}
        onClick={() => void make()}
      >
        {busy && <Loader2 className="animate-spin" />}
        {t`Make the course`}
      </Button>
    </div>
  )
}
