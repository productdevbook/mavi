import { useLingui } from "@lingui/react/macro"
import { X } from "lucide-react"

import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

/** One field a kind declares, in the shape this screen draws. */
export interface Field {
  name: string
  label: string
  type: string
  required: boolean
  options: string[]
}

/**
 * The fields a piece of content has because of what kind of thing it is.
 *
 * A course has a price and a level; a property has rooms. What is declared is
 * what is drawn, and what is written under a field the kind no longer declares
 * is kept rather than thrown away — the API keeps it, so this shows it and
 * lets somebody clear it, which is the only way it could otherwise be reached.
 */
export function ContentFields({
  fields,
  values,
  onChange,
}: {
  fields: Field[]
  values: Record<string, unknown>
  onChange: (values: Record<string, unknown>) => void
}) {
  const { t } = useLingui()

  const outlived = Object.keys(values).filter(
    (name) =>
      values[name] !== null &&
      values[name] !== undefined &&
      !fields.some((field) => field.name === name),
  )

  if (fields.length === 0 && outlived.length === 0) {
    return null
  }

  const set = (name: string, value: unknown) =>
    onChange({ ...values, [name]: value })

  // Left out of what is sent, which is how the API is told to let it go.
  const forget = (name: string) => {
    const rest = { ...values }

    delete rest[name]
    onChange(rest)
  }

  return (
    <div className="flex flex-col gap-4">
      {fields.map((field) => (
        <div key={field.name} className="flex flex-col gap-1.5">
          <Label htmlFor={`field-${field.name}`}>
            {field.label}
            {field.required && <span className="text-destructive"> *</span>}
          </Label>

          {field.type === "boolean" || field.type === "checkbox" ? (
            <Switch
              id={`field-${field.name}`}
              checked={values[field.name] === true}
              onCheckedChange={(value) => set(field.name, value)}
            />
          ) : field.type === "select" || field.type === "choice" ? (
            <Select
              value={String(values[field.name] ?? "")}
              onValueChange={(value) => set(field.name, value ?? "")}
            >
              <SelectTrigger id={`field-${field.name}`}>
                <SelectValue placeholder={t`Not chosen`} />
              </SelectTrigger>
              <SelectContent>
                {field.options.map((one) => (
                  <SelectItem key={one} value={one}>
                    {one}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : field.type === "textarea" ? (
            <Textarea
              id={`field-${field.name}`}
              rows={4}
              value={String(values[field.name] ?? "")}
              onChange={(event) => set(field.name, event.target.value)}
            />
          ) : (
            <Input
              id={`field-${field.name}`}
              type={
                field.type === "number"
                  ? "number"
                  : field.type === "date" || field.type === "moment"
                    ? "datetime-local"
                    : "text"
              }
              value={String(values[field.name] ?? "")}
              onChange={(event) =>
                set(
                  field.name,
                  field.type === "number"
                    ? Number(event.target.value)
                    : event.target.value,
                )
              }
            />
          )}
        </div>
      ))}

      {outlived.length > 0 && (
        <div className="flex flex-col gap-2 rounded-lg border border-dashed border-border p-3">
          <p className="text-xs text-muted-foreground">
            {t`Held from when this kind had fields it no longer has. They are kept as they were, and come back if the field does.`}
          </p>

          {outlived.map((name) => (
            <div key={name} className="flex items-center gap-2">
              <span className="min-w-0 flex-1 truncate font-mono text-xs">
                {name}: {String(values[name])}
              </span>
              <button
                type="button"
                aria-label={t`Forget it`}
                onClick={() => forget(name)}
              >
                <X className="size-4 text-muted-foreground" />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
