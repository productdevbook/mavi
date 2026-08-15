/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { ArrowLeft, Check, Copy, Loader2, RefreshCw, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type {
  Form,
  FormField,
  Submission,
} from "../../server/types/mavicms"
import { Button } from "@/components/ui/button"

export const Route = createFileRoute("/dashboard/forms_/$formId")({
  component: FormSubmissionsRoute,
})

/** What a value looks like in a table cell. */
function shown(value: unknown): string {
  if (value === null || value === undefined) return "—"
  if (typeof value === "boolean") return value ? "✓" : "✗"
  if (typeof value === "object") return JSON.stringify(value)
  return String(value)
}

function Snippet({ code }: { code: string }) {
  const { t } = useLingui()
  const [copied, setCopied] = React.useState(false)

  const copy = () => {
    void navigator.clipboard.writeText(code).then(
      () => {
        setCopied(true)
        setTimeout(() => setCopied(false), 1500)
      },
      () => toast.error(t`Could not copy it`)
    )
  }

  return (
    <div className="relative">
      <pre className="overflow-x-auto rounded-xl border border-border bg-muted/40 px-4 py-3 pr-12 text-xs">
        {code}
      </pre>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={t`Copy`}
        className="absolute top-2 right-2"
        onClick={copy}
      >
        {copied ? <Check /> : <Copy />}
      </Button>
    </div>
  )
}

function FormSubmissionsRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const { formId } = Route.useParams()

  const [form, setForm] = React.useState<Form | null>(null)
  const [rows, setRows] = React.useState<Submission[] | null>(null)
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    Promise.all([
      api("GET /api/forms/{id}", { path: { id: formId } }),
      every("GET /api/forms/{id}/submissions", { path: { id: formId } }),
    ])
      .then(([one, submissions]) => {
        setForm(one as Form)
        setRows(submissions)
      })
      .catch((why: unknown) => toast.error(said(why)))
  }, [formId])

  React.useEffect(load, [load])

  const markRead = async () => {
    setBusy(true)
    try {
      await api("POST /api/forms/{id}/seen", { path: { id: formId } })
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setBusy(false)
    }
  }

  const remove = async (submission: Submission) => {
    try {
      await api("DELETE /api/forms/{id}/submissions/{submission_id}", {
        path: { id: formId, submission_id: submission.id },
      })
      setRows((held) => (held ?? []).filter((row) => row.id !== submission.id))
    } catch (why) {
      toast.error(said(why))
    }
  }

  if (!form || !rows) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const fields = (form.fields as FormField[] | null) ?? []

  const endpoint = `${window.location.origin}/api/sites/forms/${form.slug}/submissions`

  const example = JSON.stringify(
    {
      answers: Object.fromEntries(
        fields.map((field) => [
          field.key,
          field.kind === "boolean"
            ? true
            : field.kind === "number"
              ? 1
              : field.kind === "email"
                ? "somebody@example.test"
                : field.kind === "choice"
                  ? (field.options?.[0] ?? "")
                  : field.label,
        ]),
      ),
    },
    null,
    2,
  )

  return (
    <>
      <Button
        variant="ghost"
        size="sm"
        className="mb-4 -ml-2"
        onClick={() => void navigate({ to: "/dashboard/forms" })}
      >
        <ArrowLeft /> {t`Forms`}
      </Button>

      <div className="mb-6 flex items-start justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold">{form.name}</h1>
          <p className="text-sm text-muted-foreground">
            {t`${form.submissions} received, ${form.unseen} not yet opened.`}
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={load}>
            <RefreshCw /> {t`Refresh`}
          </Button>
          <Button
            size="sm"
            disabled={busy || form.unseen === 0}
            onClick={() => void markRead()}
          >
            <Check /> {t`Mark all read`}
          </Button>
        </div>
      </div>

      <div className="mb-8 flex flex-col gap-2">
        <h2 className="text-sm font-medium">{t`How to send to it`}</h2>
        <p className="text-sm text-muted-foreground">
          {t`Post JSON to this address from your own pages or software. No account is needed, and only the fields above are accepted.`}
        </p>
        <Snippet
          code={`curl -X POST ${endpoint} \\\n  -H 'Content-Type: application/json' \\\n  -d '${example.replace(/\n\s*/g, " ")}'`}
        />
      </div>

      {rows.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border py-12 text-center text-sm text-muted-foreground">
          {t`Nothing has come in yet`}
        </p>
      ) : (
        <div className="overflow-x-auto rounded-xl border border-border">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border text-left">
                <th className="px-4 py-2 font-medium whitespace-nowrap">
                  {t`When`}
                </th>
                {fields.map((field) => (
                  <th
                    key={field.key}
                    className="px-4 py-2 font-medium whitespace-nowrap"
                  >
                    {field.label}
                  </th>
                ))}
                <th className="px-4 py-2" />
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr
                  key={row.id}
                  className="border-b border-border last:border-0"
                >
                  <td className="px-4 py-2 whitespace-nowrap text-muted-foreground">
                    {!row.seen_at && (
                      <span className="surface-mark mr-2 inline-block size-1.5 rounded-full align-middle" />
                    )}
                    {new Date(row.created_at).toLocaleString()}
                  </td>
                  {fields.map((field) => (
                    <td key={field.key} className="max-w-xs px-4 py-2">
                      <span className="block truncate">
                        {shown(
                          (row.answers as Record<string, unknown> | null)?.[
                            field.key
                          ],
                        )}
                      </span>
                    </td>
                  ))}
                  <td className="px-4 py-2">
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t`Remove`}
                      onClick={() => void remove(row)}
                    >
                      <Trash2 />
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  )
}
