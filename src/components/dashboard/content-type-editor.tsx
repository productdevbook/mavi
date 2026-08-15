import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { useLanguages } from "@/lib/use-languages"
import type { ContentType, FieldKind } from "../../../server/types/mavicms"
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

/** One field a kind declares, as the API keeps it. */
interface Declared {
  name: string
  label?: string | null
  kind: FieldKind
  required: boolean
  choices: string[]
}

/** What a kind is called in one language. */
interface Called {
  name?: string
  plural?: string
}

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
  const { languages } = useLanguages()

  const [name, setName] = React.useState(kind?.name ?? "")
  const [plural, setPlural] = React.useState(kind?.plural ?? "")
  const [names, setNames] = React.useState<Record<string, Called>>(
    (kind?.names as Record<string, Called> | null) ?? {},
  )
  const [fields, setFields] = React.useState<Declared[]>(
    (kind?.fields as Declared[] | null) ?? [],
  )
  const [saving, setSaving] = React.useState(false)

  const kinds: { value: FieldKind; label: string }[] = [
    { value: "text", label: t`Text` },
    { value: "number", label: t`A number` },
    { value: "boolean", label: t`Yes or no` },
    { value: "moment", label: t`A date` },
    { value: "choice", label: t`One of a few` },
  ]

  const save = async () => {
    setSaving(true)

    try {
      const body = {
        name,
        plural: plural || null,
        names,
        fields,
      }

      const saved = kind
        ? await api("PUT /api/content-types/{key}", {
            path: { key: kind.key },
            body,
          })
        : await api("POST /api/content-types", {
            body: { ...body, key: keyed(name) },
          })

      onDone(saved as ContentType)
    } catch (why) {
      toast.error(said(why))
      setSaving(false)
    }
  }

  const change = (at: number, field: Partial<Declared>) =>
    setFields(
      fields.map((one, index) => (index === at ? { ...one, ...field } : one)),
    )

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <div>
        <h1 className="text-lg font-semibold">
          {kind ? (kind.plural ?? kind.name) : t`A new kind`}
        </h1>
        <p className="text-sm text-muted-foreground">
          {kind
            ? t`What one of these is made of, beyond its title and its text.`
            : t`Give it a name and say what one is made of. The key it answers on is made from the name and never changes afterwards, because a front end will be asking for it.`}
        </p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="kind-name">{t`One of them is called`}</Label>
          <Input
            id="kind-name"
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder={t`Course`}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="kind-plural">{t`Several are called`}</Label>
          <Input
            id="kind-plural"
            value={plural}
            onChange={(event) => setPlural(event.target.value)}
            placeholder={t`Courses`}
          />
        </div>
      </div>

      {/* Only worth asking about on a site that writes in more than one: with
          one language the two boxes above are the whole answer. */}
      {languages.length > 1 && (
        <div className="flex flex-col gap-3 rounded-xl border border-border p-3">
          <div>
            <p className="text-sm font-medium">{t`In each language`}</p>
            <p className="text-xs text-muted-foreground">
              {t`What the panel calls this for somebody reading it in that language. Left empty, it is called what it is called above.`}
            </p>
          </div>

          {languages.map((language) => (
            <div
              key={language.code}
              className="grid gap-2 sm:grid-cols-[4rem_1fr_1fr]"
            >
              <span className="self-center text-xs text-muted-foreground">
                {language.name}
              </span>
              <Input
                aria-label={t`One of them, in ${language.code}`}
                value={names[language.code]?.name ?? ""}
                placeholder={name}
                onChange={(event) =>
                  setNames({
                    ...names,
                    [language.code]: {
                      ...names[language.code],
                      name: event.target.value,
                    },
                  })
                }
              />
              <Input
                aria-label={t`Several, in ${language.code}`}
                value={names[language.code]?.plural ?? ""}
                placeholder={plural || name}
                onChange={(event) =>
                  setNames({
                    ...names,
                    [language.code]: {
                      ...names[language.code],
                      plural: event.target.value,
                    },
                  })
                }
              />
            </div>
          ))}
        </div>
      )}

      {kind && (
        <p className="text-sm text-muted-foreground">
          {t`A front end asks for these with ?type=${kind.key}`}
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
                  name: "",
                  label: "",
                  kind: "text",
                  required: false,
                  choices: [],
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
                    name: field.name || keyed(label),
                  })
                }}
              />
              <Input
                value={field.name}
                placeholder={t`Its key`}
                className="w-40 font-mono"
                onChange={(event) => change(index, { name: event.target.value })}
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
                value={field.choices.join(", ")}
                placeholder={t`The choices, separated by commas`}
                onChange={(event) =>
                  change(index, {
                    choices: event.target.value
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
                  onCheckedChange={(value) => change(index, { required: value })}
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
