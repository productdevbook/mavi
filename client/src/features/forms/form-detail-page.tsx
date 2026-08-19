import * as React from "react"
import { useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { ArrowLeft, Check, Copy, RefreshCw, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { Form, FormField, FormSubmission as Submission } from "@api"
import {
  DashboardEmpty,
  DashboardLoading,
  DashboardPageHeader,
} from "@/components/dashboard/dashboard-page"
import { Button } from "@/components/ui/button"

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

export function FormDetailPage({ formId }: { formId: string }) {
  const { t } = useLingui()
  const navigate = useNavigate()

  const [form, setForm] = React.useState<Form | null>(null)
  const [rows, setRows] = React.useState<Submission[] | null>(null)

  const load = React.useCallback(() => {
    Promise.all([
      api("forms.read", { path: { id: formId } }),
      every("forms.submissions.list", { path: { id: formId }, query: {} }),
    ])
      .then(([one, submissions]) => {
        setForm(one)
        setRows(submissions)
      })
      .catch((why: unknown) => toast.error(apiMessage(why)))
  }, [formId])

  React.useEffect(load, [load])

  const remove = async (submission: Submission) => {
    try {
      await api("forms.submissions.delete", {
        path: { id: submission.id },
      })
      setRows((held) => (held ?? []).filter((row) => row.id !== submission.id))
    } catch (why) {
      toast.error(apiMessage(why))
    }
  }

  if (!form || !rows) {
    return <DashboardLoading />
  }

  const fields = (form.fields as FormField[] | null) ?? []

  const endpoint = `${window.location.origin}/public/v1/forms/${form.slug}/submissions`

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
        ])
      ),
    },
    null,
    2
  )

  return (
    <div className="flex flex-col gap-6">
      <Button
        variant="ghost"
        size="sm"
        className="-ml-2 self-start"
        onClick={() => void navigate({ to: "/dashboard/forms" })}
      >
        <ArrowLeft /> {t`Forms`}
      </Button>

      <DashboardPageHeader
        title={form.name}
        description={t`${rows.length} received.`}
        actions={
          <Button variant="outline" size="sm" onClick={load}>
            <RefreshCw /> {t`Refresh`}
          </Button>
        }
      />

      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-medium">{t`How to send to it`}</h2>
        <p className="text-sm text-muted-foreground">
          {t`Post JSON to this address from your own pages or software. No account is needed, and only the fields above are accepted.`}
        </p>
        <Snippet
          code={`curl -X POST ${endpoint} \\\n  -H 'Content-Type: application/json' \\\n  -d '${example.replace(/\n\s*/g, " ")}'`}
        />
      </section>

      {rows.length === 0 ? (
        <DashboardEmpty title={t`Nothing has come in yet`} />
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
                          ]
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
    </div>
  )
}
