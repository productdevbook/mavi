/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { Link, createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Inbox, Loader2, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api, every } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import type { Form, FormField } from "@api"
import { DashboardPageHeader } from "@/components/dashboard/dashboard-page"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
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
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"

export const Route = createFileRoute("/dashboard/forms")({
  component: FormsRoute,
})

type Draft = {
  id: string | null
  name: string
  slug: string
  fields: FormField[]
  open: boolean
  kept_days: number
}

/** A key out of a label: lower-case, dashes for gaps. */
function keyed(label: string): string {
  return label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
}

function FormsRoute() {
  const { t } = useLingui()

  const [forms, setForms] = React.useState<Form[] | null>(null)
  const [draft, setDraft] = React.useState<Draft | null>(null)
  const [saving, setSaving] = React.useState(false)
  const [removing, setRemoving] = React.useState<Form | null>(null)

  const load = React.useCallback(() => {
    every("GET /api/forms")
      .then(setForms)
      .catch((why: unknown) => {
        toast.error(said(why))
        setForms((held) => held ?? [])
      })
  }, [])

  React.useEffect(load, [load])

  const save = async () => {
    if (!draft) return

    setSaving(true)

    try {
      if (draft.id) {
        await api("PATCH /api/forms/{id}", {
          path: { id: draft.id },
          body: {
            name: draft.name.trim(),
            fields: draft.fields,
            open: draft.open,
            kept_days: draft.kept_days,
          },
        })
      } else {
        await api("POST /api/forms", {
          body: {
            slug: draft.slug.trim() || keyed(draft.name),
            name: draft.name.trim(),
            fields: draft.fields,
            kept_days: draft.kept_days,
          },
        })
      }

      setDraft(null)
      load()
      toast.success(t`Saved`)
    } catch (why) {
      toast.error(said(why))
    } finally {
      setSaving(false)
    }
  }

  const remove = async () => {
    if (!removing) return

    try {
      await api("DELETE /api/forms/{id}", { path: { id: removing.id } })
      load()
    } catch (why) {
      toast.error(said(why))
    } finally {
      setRemoving(null)
    }
  }

  const edit = (form: Form) =>
    setDraft({
      id: form.id,
      name: form.name,
      slug: form.slug,
      fields: form.fields ?? [],
      open: form.open,
      kept_days: form.kept_days,
    })

  const ready =
    draft !== null &&
    draft.name.trim().length > 0 &&
    draft.fields.every((field) => field.key.length > 0)

  return (
    <>
      <DashboardPageHeader
        className="mb-6"
        title={t`Forms`}
        description={t`Say what a form accepts here, then have your own pages post to it. What it does not accept is refused rather than kept.`}
        actions={
          <Button
            onClick={() =>
              setDraft({
                id: null,
                name: "",
                slug: "",
                fields: [
                  {
                    key: "name",
                    label: t`Name`,
                    required: true,
                    kind: "text",
                    options: [],
                  },
                ],
                open: true,
                kept_days: 365,
              })
            }
          >
            <Plus /> {t`New form`}
          </Button>
        }
      />

      {!forms ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : forms.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border py-12 text-center text-sm text-muted-foreground">
          {t`No forms yet`}
        </p>
      ) : (
        <div className="flex max-w-3xl flex-col divide-y divide-border rounded-xl border border-border">
          {forms.map((form) => (
            <div key={form.id} className="flex items-center gap-3 px-4 py-3">
              <Inbox className="size-4 shrink-0 text-muted-foreground" />

              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">
                  {form.name}
                  {!form.open && (
                    <span className="ml-2 text-xs font-normal text-muted-foreground">
                      {t`switched off`}
                    </span>
                  )}
                </p>
                <p className="truncate font-mono text-xs text-muted-foreground">
                  /api/forms/{form.slug}/filled
                </p>
              </div>

              <Link
                to="/dashboard/forms/$formId"
                params={{ formId: form.id }}
                className="flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-sm font-medium text-muted-foreground hover:bg-muted hover:text-foreground"
              >
                {t`Submissions`}
              </Link>

              <Button variant="outline" size="sm" onClick={() => edit(form)}>
                {t`Edit`}
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Remove`}
                onClick={() => setRemoving(form)}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      <Dialog
        open={draft !== null}
        onOpenChange={(open) => !open && setDraft(null)}
      >
        <DialogContent className="max-h-[90svh] overflow-y-auto sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>
              {draft?.id ? t`Edit the form` : t`New form`}
            </DialogTitle>
            <DialogDescription>
              {t`What comes in is kept as it was sent, so changing the fields later never rewrites it.`}
            </DialogDescription>
          </DialogHeader>

          {draft && (
            <div className="flex flex-col gap-5">
              <div className="flex flex-col gap-2">
                <Label htmlFor="form-name">{t`Name`}</Label>
                <Input
                  id="form-name"
                  value={draft.name}
                  onChange={(event) => {
                    const name = event.target.value

                    // Only while it is new: changing the address of a form
                    // already in use would stop whatever posts to it.
                    setDraft({
                      ...draft,
                      name,
                      slug: draft.id ? draft.slug : keyed(name),
                    })
                  }}
                />
                <p className="font-mono text-xs text-muted-foreground">
                  /api/forms/{draft.slug || keyed(draft.name)}/filled
                </p>
              </div>

              <Fields
                fields={draft.fields}
                onChange={(fields) => setDraft({ ...draft, fields })}
              />

              <div className="flex items-center justify-between gap-4 rounded-lg border border-border px-4 py-3">
                <div>
                  <p className="text-sm font-medium">{t`Taking answers`}</p>
                  <p className="text-xs text-muted-foreground">
                    {t`Switched off, the form refuses what is sent to it.`}
                  </p>
                </div>
                <Switch
                  checked={draft.open}
                  onCheckedChange={(value) =>
                    setDraft({ ...draft, open: value })
                  }
                />
              </div>

              <div className="flex flex-col gap-2">
                <Label htmlFor="form-retention">{t`Kept for, in days`}</Label>
                <Input
                  id="form-retention"
                  inputMode="numeric"
                  value={String(draft.kept_days)}
                  onChange={(event) =>
                    setDraft({
                      ...draft,
                      kept_days: Number(event.target.value) || 0,
                    })
                  }
                />
                <p className="text-xs text-muted-foreground">
                  {t`After this, what came in is deleted by the machine itself.`}
                </p>
              </div>
            </div>
          )}

          <DialogFooter>
            <Button variant="outline" onClick={() => setDraft(null)}>
              {t`Cancel`}
            </Button>
            <Button disabled={!ready || saving} onClick={() => void save()}>
              {saving && <Loader2 className="animate-spin" />}
              {t`Save`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={removing !== null}
        onOpenChange={(open) => !open && setRemoving(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Remove this form?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`What has already come in goes with it.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void remove()}>
              {t`Remove`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

/**
 * What a form asks for.
 *
 * The key is what an answer arrives under and is not changed once anything has
 * arrived: what came in is kept as it was sent, so a key renamed here would
 * leave every old answer filed under a name nothing looks for.
 */
function Fields({
  fields,
  onChange,
}: {
  fields: FormField[]
  onChange: (fields: FormField[]) => void
}) {
  const { t } = useLingui()

  const kinds: { value: FormField["kind"]; label: string }[] = [
    { value: "text", label: t`A line of text` },
    { value: "long", label: t`More than a line` },
    { value: "email", label: t`An email address` },
    { value: "number", label: t`A number` },
    { value: "choice", label: t`One of a few` },
    { value: "boolean", label: t`Yes or no` },
  ]

  const change = (at: number, field: Partial<FormField>) =>
    onChange(
      fields.map((one, index) => (index === at ? { ...one, ...field } : one)),
    )

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <Label>{t`What it asks for`}</Label>
        <Button
          variant="outline"
          size="sm"
          onClick={() =>
            onChange([
              ...fields,
              { key: "", label: "", required: false, kind: "text", options: [] },
            ])
          }
        >
          <Plus /> {t`Another`}
        </Button>
      </div>

      {fields.map((field, index) => (
        <div
          key={index}
          className="flex flex-col gap-2 rounded-lg border border-border p-3"
        >
          <div className="flex flex-wrap gap-2">
            <Input
              value={field.label}
              placeholder={t`What it is called`}
              className="min-w-40 flex-1"
              onChange={(event) => {
                const label = event.target.value

                change(index, {
                  label,
                  key: field.key || keyed(label),
                })
              }}
            />
            <Input
              value={field.key}
              placeholder={t`Its key`}
              className="w-40 font-mono"
              onChange={(event) => change(index, { key: event.target.value })}
            />
            <Select
              value={field.kind}
              onValueChange={(value) =>
                change(index, {
                  kind: (value as FormField["kind"]) ?? "text",
                })
              }
            >
              <SelectTrigger className="w-44">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {kinds.map((kind) => (
                  <SelectItem key={kind.value} value={kind.value}>
                    {kind.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {field.kind === "choice" && (
            <Input
              value={(field.options ?? []).join(", ")}
              placeholder={t`The choices, separated by commas`}
              onChange={(event) =>
                change(index, {
                  options: event.target.value
                    .split(",")
                    .map((one) => one.trim())
                    .filter(Boolean),
                })
              }
            />
          )}

          <div className="flex items-center justify-between gap-3">
            <label className="flex items-center gap-2 text-sm">
              <Switch
                checked={field.required}
                onCheckedChange={(value) =>
                  change(index, { required: value })
                }
              />
              {t`Has to be filled in`}
            </label>

            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t`Remove`}
              onClick={() =>
                onChange(fields.filter((_, which) => which !== index))
              }
            >
              <Trash2 />
            </Button>
          </div>
        </div>
      ))}
    </div>
  )
}
