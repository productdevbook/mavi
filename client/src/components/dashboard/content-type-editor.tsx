import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/api"
import { apiMessage } from "@/lib/auth"
import type { ContentFieldKind, ContentType, ContentTypeField } from "@api"
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

type FieldKind = ContentFieldKind

/** A key out of a name: lower-case, underscores for gaps. */
function keyed(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
}

/**
 * What one kind of thing is called and made of.
 *
 * The key it answers on is made from the name when it is new and never
 * changes afterwards: a front end asks for `?type=course`, and a key that
 * moved would be a site whose pages stopped finding anything.
 */
export function ContentTypeEditor({
  kind,
  onDone,
}: {
  kind: ContentType | null
  onDone: (saved?: ContentType) => void
}) {
  const { t } = useLingui()

  const [name, setName] = React.useState(kind?.name ?? "")
  const [fields, setFields] = React.useState<ContentTypeField[]>(
    kind?.fields ?? []
  )
  const [saving, setSaving] = React.useState(false)

  const kinds: { value: FieldKind; label: string }[] = [
    { value: "text", label: t`Text` },
    { value: "long", label: t`Long text` },
    { value: "number", label: t`A number` },
    { value: "boolean", label: t`Yes or no` },
    { value: "email", label: t`Email` },
    { value: "choice", label: t`One of a few` },
  ]

  const save = async () => {
    setSaving(true)

    try {
      const kindName = kind ? kind.kind : keyed(name)
      const saved = await api("content_types.upsert", {
        path: { kind: kindName },
        body: { name, fields },
      })

      onDone(saved)
    } catch (why) {
      toast.error(apiMessage(why))
      setSaving(false)
    }
  }

  const change = (at: number, field: Partial<ContentTypeField>) =>
    setFields(
      fields.map((one, index) => (index === at ? { ...one, ...field } : one))
    )

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <div>
        <h1 className="text-lg font-semibold">
          {kind ? kind.name : t`A new kind`}
        </h1>
        <p className="text-sm text-muted-foreground">
          {kind
            ? t`What one of these is made of, beyond its title and its text.`
            : t`Give it a name and say what one is made of. The key it answers on is made from the name and never changes afterwards, because a front end will be asking for it.`}
        </p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="kind-name">{t`What it is called`}</Label>
          <Input
            id="kind-name"
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder={t`Course`}
          />
        </div>
      </div>

      {kind && (
        <p className="text-sm text-muted-foreground">
          {t`A front end asks for these with ?kind=${kind.kind}`}
        </p>
      )}

      <div className="flex flex-col gap-3">
        <div className="flex items-center justify-between">
          <Label>{t`What one of these holds`}</Label>
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              setFields([
                ...fields,
                {
                  key: "",
                  label: "",
                  kind: "text",
                  required: false,
                  options: [],
                },
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
                value={field.label ?? ""}
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
                  change(index, { kind: (value as FieldKind) ?? "text" })
                }
              >
                <SelectTrigger className="w-40">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {kinds.map((one) => (
                    <SelectItem key={one.value} value={one.value}>
                      {one.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            {field.kind === "choice" && (
              <Input
                value={field.options.join(", ")}
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
                  setFields(fields.filter((_, which) => which !== index))
                }
              >
                <Trash2 />
              </Button>
            </div>
          </div>
        ))}
      </div>

      <div className="flex gap-2">
        <Button disabled={!name.trim() || saving} onClick={() => void save()}>
          {saving && <Loader2 className="animate-spin" />}
          {t`Save`}
        </Button>
        <Button variant="outline" onClick={() => onDone()}>
          {t`Cancel`}
        </Button>
      </div>
    </div>
  )
}
